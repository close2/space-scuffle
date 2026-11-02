//! Real WGSL composition and validation for shipped shaders, without a GPU.
//!
//! # Responsibility
//! Owns the path from a file in `assets/shaders` to a validated [`naga::Module`] that a test can
//! inspect. Shipped shaders `#import` Bevy modules, so `naga::front::wgsl::parse_str` alone cannot
//! read them. This module repeats the two steps of Bevy's private `ShaderCache`: register each
//! imported module with [`Composer`], then call [`Composer::make_naga_module`].
//!
//! # How it fits
//! The imported sources are never copied or stubbed. A renderer-less Bevy app builds the normal
//! plugin group with `WgpuSettings { backends: None }`. Plugins still register their embedded WGSL
//! in `Assets<Shader>`, so composition uses the locked Bevy sources that the game runs.
//!
//! # Invariants
//! - [`SHADER_VARIANTS`] start from the shader defs production logged while compiling these
//!   materials (`RUST_LOG=bevy_render::render_resource::pipeline_cache=debug`).
//! - [`compose_and_validate`] always runs a separate strict [`Validator`] pass, even though the
//!   composer validates as well, so the [`ModuleInfo`] handed back is proof of that pass.
//! - Probes inspect IR only. They never search source text, so comments cannot satisfy them.

#![allow(clippy::expect_used, clippy::panic)]

use bevy::audio::AudioPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::render::render_resource::{Shader, ShaderDefVal, ShaderImport};
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::render::RenderPlugin;
use bevy::winit::WinitPlugin;
use naga::valid::{Capabilities, ModuleInfo, ValidationFlags, Validator};
use naga::{Block, Expression, Function, Handle, Module, ShaderStage, Statement, TypeInner};
use naga_oil::compose::{Composer, NagaModuleDescriptor, ShaderDefValue};
use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

/// File name of the only shader drawn on meshes with a second UV set.
const TERRAIN_SHADER: &str = "terrain.wgsl";
/// Binding of the tonemapping LUT texture in Bevy's mesh view layout, as logged by production.
const LUT_TEXTURE_BINDING: u32 = 23;
/// Binding of the sampler paired with [`LUT_TEXTURE_BINDING`].
const LUT_SAMPLER_BINDING: u32 = 24;
/// `max_storage_buffers_per_shader_stage` reported by lavapipe; Bevy appends it to every shader.
const STORAGE_BUFFER_BINDINGS: u32 = 1_015_808;

/// One shader-def combination that Bevy's mesh pipeline can request.
#[derive(Debug, Clone, Copy)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each field mirrors one independent bit of Bevy's MeshPipelineKey"
)]
pub struct ShaderVariant {
    /// Human-readable name used in failure messages.
    pub name: &'static str,
    /// Adds `MULTISAMPLED`, selecting `texture_depth_multisampled_2d` prepass bindings.
    pub multisampled: bool,
    /// Adds `DEPTH_PREPASS`, enabling the prepass depth reads in volume shaders.
    pub depth_prepass: bool,
    /// Adds `TONEMAP_IN_SHADER` with the production Tony `McMapface` LUT and `DEBAND_DITHER`.
    pub tonemap: bool,
    /// Adds `PREMULTIPLY_ALPHA` and `BLEND_PREMULTIPLIED_ALPHA`, used by `AlphaMode::Blend`.
    pub premultiplied_blend: bool,
}

/// Every combination of the optional defs that shipped shaders branch on.
///
/// The first two entries are the defs logged by the real client. The others are the combinations a
/// camera can select by turning off MSAA, the depth prepass or in-shader tonemapping (HDR cameras).
pub const SHADER_VARIANTS: [ShaderVariant; 5] = [
    ShaderVariant {
        name: "production_opaque",
        multisampled: true,
        depth_prepass: true,
        tonemap: true,
        premultiplied_blend: false,
    },
    ShaderVariant {
        name: "production_blended",
        multisampled: true,
        depth_prepass: true,
        tonemap: true,
        premultiplied_blend: true,
    },
    ShaderVariant {
        name: "single_sample",
        multisampled: false,
        depth_prepass: true,
        tonemap: true,
        premultiplied_blend: false,
    },
    ShaderVariant {
        name: "no_prepass",
        multisampled: true,
        depth_prepass: false,
        tonemap: true,
        premultiplied_blend: false,
    },
    ShaderVariant {
        name: "hdr_no_tonemap",
        multisampled: true,
        depth_prepass: true,
        tonemap: false,
        premultiplied_blend: false,
    },
];

impl ShaderVariant {
    /// Returns the shader defs for this variant, in the form Bevy passes to the composer.
    ///
    /// `path` selects mesh-layout defs: terrain meshes alone carry a second UV set, which the
    /// production terrain pipeline reports as `VERTEX_UVS_B`.
    fn defs(self, path: &str) -> Vec<ShaderDefVal> {
        let mut defs: Vec<ShaderDefVal> = vec![
            "MESH_PIPELINE".into(),
            "VERTEX_OUTPUT_INSTANCE_INDEX".into(),
            "VERTEX_POSITIONS".into(),
            "VERTEX_NORMALS".into(),
            "VERTEX_UVS".into(),
            "VERTEX_UVS_A".into(),
            "VIEW_PROJECTION_PERSPECTIVE".into(),
            "SHADOW_FILTER_METHOD_GAUSSIAN".into(),
            "MULTIPLE_LIGHT_PROBES_IN_ARRAY".into(),
            "IRRADIANCE_VOLUMES_ARE_USABLE".into(),
            ShaderDefVal::Int("SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS".into(), 8),
            ShaderDefVal::UInt(
                "AVAILABLE_STORAGE_BUFFER_BINDINGS".into(),
                STORAGE_BUFFER_BINDINGS,
            ),
        ];
        if path == TERRAIN_SHADER {
            defs.push("VERTEX_UVS_B".into());
        }
        if self.multisampled {
            defs.push("MULTISAMPLED".into());
        }
        if self.depth_prepass {
            defs.push("DEPTH_PREPASS".into());
        }
        if self.premultiplied_blend {
            defs.push("PREMULTIPLY_ALPHA".into());
            defs.push("BLEND_PREMULTIPLIED_ALPHA".into());
        }
        if self.tonemap {
            defs.push("TONEMAP_IN_SHADER".into());
            defs.push("TONEMAP_METHOD_TONY_MC_MAPFACE".into());
            defs.push("DEBAND_DITHER".into());
            defs.push(ShaderDefVal::UInt(
                "TONEMAPPING_LUT_TEXTURE_BINDING_INDEX".into(),
                LUT_TEXTURE_BINDING,
            ));
            defs.push(ShaderDefVal::UInt(
                "TONEMAPPING_LUT_SAMPLER_BINDING_INDEX".into(),
                LUT_SAMPLER_BINDING,
            ));
        }
        defs
    }
}

/// A composed shader and the validator's proof that it is well formed.
#[derive(Debug)]
pub struct ValidatedShader {
    /// The fully composed module, including imported Bevy items.
    pub module: Module,
    /// Validator output. Holding it means the strict validation pass succeeded.
    pub info: ModuleInfo,
}

/// Returns every WGSL module registered by Bevy's default plugins, once per process.
fn bevy_shader_library() -> &'static [Shader] {
    static LIBRARY: OnceLock<Vec<Shader>> = OnceLock::new();
    LIBRARY.get_or_init(|| {
        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .build()
                .disable::<LogPlugin>()
                .disable::<WinitPlugin>()
                .disable::<AudioPlugin>()
                .set(WindowPlugin {
                    primary_window: None,
                    ..default()
                })
                .set(RenderPlugin {
                    render_creation: RenderCreation::Automatic(WgpuSettings {
                        backends: None,
                        ..default()
                    }),
                    ..default()
                }),
        );
        // `RenderPlugin` registers `bevy_render::maths` in `finish`, not `build`.
        app.finish();
        app.cleanup();
        let library: Vec<Shader> = app
            .world()
            .resource::<Assets<Shader>>()
            .iter()
            .map(|(_, shader)| shader.clone())
            .collect();
        assert!(
            library.len() > 50,
            "Bevy's plugins registered only {} shaders; imports could not be resolved",
            library.len()
        );
        library
    })
}

/// Registers `import` and everything it imports, in dependency order.
///
/// Mirrors `ShaderCache::add_import_to_composer`. An import with no library entry is left for the
/// composer to report, so a misspelt `#import` becomes a named composition error.
fn add_import(
    composer: &mut Composer,
    library: &HashMap<&ShaderImport, &Shader>,
    import: &ShaderImport,
) -> Result<(), String> {
    if composer.contains_module(&import.module_name()) {
        return Ok(());
    }
    let Some(shader) = library.get(import) else {
        return Ok(());
    };
    for nested in &shader.imports {
        add_import(composer, library, nested)?;
    }
    composer
        .add_composable_module(naga_oil::compose::ComposableModuleDescriptor::from(*shader))
        .map(|_| ())
        .map_err(|error| format!("{}: {}", shader.path, error.emit_to_string(composer)))
}

/// Capabilities every target device has, matching a Bevy `get_capabilities` result with no optional
/// wgpu features. Shipped shaders must not rely on anything beyond this baseline.
fn baseline_capabilities() -> Capabilities {
    Capabilities::MULTISAMPLED_SHADING | Capabilities::CUBE_ARRAY_TEXTURES
}

/// Composes `source` with real Bevy imports under `variant`, then validates it strictly.
///
/// `path` labels diagnostics and selects mesh-layout defs, as described on `ShaderVariant::defs`.
///
/// # Errors
/// Returns the rendered composer or validator diagnostic when `source` does not parse, names an
/// unknown import, or fails validation.
pub fn compose_and_validate(
    path: &str,
    source: &str,
    variant: ShaderVariant,
) -> Result<ValidatedShader, String> {
    let library: HashMap<&ShaderImport, &Shader> = bevy_shader_library()
        .iter()
        .filter(|shader| matches!(shader.import_path(), ShaderImport::Custom(_)))
        .map(|shader| (shader.import_path(), shader))
        .collect();
    let shader = Shader::from_wgsl(source.to_owned(), path.to_owned());
    let mut composer = Composer::default().with_capabilities(baseline_capabilities());
    for import in shader.imports() {
        add_import(&mut composer, &library, import)?;
    }
    let shader_defs: HashMap<String, ShaderDefValue> = variant
        .defs(path)
        .into_iter()
        .map(|def| match def {
            ShaderDefVal::Bool(key, value) => (key, ShaderDefValue::Bool(value)),
            ShaderDefVal::Int(key, value) => (key, ShaderDefValue::Int(value)),
            ShaderDefVal::UInt(key, value) => (key, ShaderDefValue::UInt(value)),
        })
        .collect();
    let module = composer
        .make_naga_module(NagaModuleDescriptor {
            shader_defs,
            ..NagaModuleDescriptor::from(&shader)
        })
        .map_err(|error| {
            format!(
                "{path} [{}]: {}",
                variant.name,
                error.emit_to_string(&composer)
            )
        })?;
    let info = Validator::new(ValidationFlags::all(), baseline_capabilities())
        .validate(&module)
        .map_err(|error| {
            format!(
                "{path} [{}]: {}",
                variant.name,
                error.emit_to_string(source)
            )
        })?;
    Ok(ValidatedShader { module, info })
}

/// Reads a shipped shader from `assets/shaders`, the directory the game loads from.
pub fn shipped_source(file_name: &str) -> String {
    let path = client::resolve_assets_dir().join("shaders").join(file_name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()))
}

/// Composes a shipped shader under the production opaque variant, panicking with the diagnostic.
pub fn production_module(file_name: &str) -> ValidatedShader {
    compose_and_validate(file_name, &shipped_source(file_name), SHADER_VARIANTS[0])
        .unwrap_or_else(|error| panic!("{error}"))
}

impl ValidatedShader {
    /// Returns the function of the entry point `name` for `stage`.
    pub fn entry_point(&self, stage: ShaderStage, name: &str) -> &Function {
        self.module
            .entry_points
            .iter()
            .find(|entry| entry.stage == stage && entry.name == name)
            .map_or_else(
                || panic!("no {stage:?} entry point named {name}"),
                |entry| &entry.function,
            )
    }

    /// Returns the global variable bound at `@group(group) @binding(binding)`.
    pub fn global_at(&self, group: u32, binding: u32) -> &naga::GlobalVariable {
        self.module
            .global_variables
            .iter()
            .map(|(_, global)| global)
            .find(|global| {
                global
                    .binding
                    .as_ref()
                    .is_some_and(|bound| bound.group == group && bound.binding == binding)
            })
            .unwrap_or_else(|| panic!("no global at @group({group}) @binding({binding})"))
    }

    /// Returns the name and byte span of the uniform struct at the given binding.
    pub fn uniform_struct_at(&self, group: u32, binding: u32) -> (String, usize) {
        let global = self.global_at(group, binding);
        assert_eq!(
            global.space,
            naga::AddressSpace::Uniform,
            "@group({group}) @binding({binding}) must be a uniform"
        );
        let ty = &self.module.types[global.ty];
        let TypeInner::Struct { span, .. } = ty.inner else {
            panic!("@group({group}) @binding({binding}) must have a struct type");
        };
        (
            ty.name.clone().expect("invariant: WGSL structs are named"),
            span as usize,
        )
    }

    /// Returns the value of the module-scope `u32` constant `name`, if the module retains it.
    pub fn u32_constant(&self, name: &str) -> Option<u32> {
        let (_, constant) = self
            .module
            .constants
            .iter()
            .find(|(_, constant)| constant.name.as_deref() == Some(name))?;
        match self.module.global_expressions[constant.init] {
            Expression::Literal(naga::Literal::U32(value)) => Some(value),
            _ => None,
        }
    }

    /// Returns the names of all constants that `function` reads.
    pub fn constants_read_by(&self, function: &Function) -> BTreeSet<String> {
        function
            .expressions
            .iter()
            .filter_map(|(_, expression)| match expression {
                Expression::Constant(handle) => self.module.constants[*handle].name.clone(),
                _ => None,
            })
            .collect()
    }

    /// Returns `true` when `function` reads `member` of a struct stored in any global variable.
    ///
    /// Imported names are mangled by composition, but struct member names are preserved.
    pub fn reads_global_member(&self, function: &Function, member: &str) -> bool {
        function.expressions.iter().any(|(_, expression)| {
            let Expression::AccessIndex { base, index } = expression else {
                return false;
            };
            let Expression::GlobalVariable(global) = function.expressions[*base] else {
                return false;
            };
            let ty = self.module.global_variables[global].ty;
            let TypeInner::Struct { members, .. } = &self.module.types[ty].inner else {
                return false;
            };
            members.get(*index as usize).and_then(|m| m.name.as_deref()) == Some(member)
        })
    }

    /// Returns `true` when `caller` calls the function named `callee` directly.
    pub fn calls(&self, caller: &Function, callee: &str) -> bool {
        let mut found = false;
        visit_statements(&caller.body, &mut |statement| {
            if let Statement::Call { function, .. } = statement {
                found |= self.module.functions[*function].name.as_deref() == Some(callee);
            }
        });
        found
    }

    /// Returns `true` when `function` reads component `index` of `member` of a struct argument.
    ///
    /// `.x` is component 0 and `.y` is component 1, so this tells `in.uv.y` from `in.uv.x`. A read
    /// of the whole member, such as `in.uv * 3.0`, is not a read of one component.
    pub fn reads_argument_component(
        &self,
        function: &Function,
        member: &str,
        component: u32,
    ) -> bool {
        function.expressions.iter().any(|(_, expression)| {
            let Expression::AccessIndex { base, index } = *expression else {
                return false;
            };
            if index != component {
                return false;
            }
            let Expression::AccessIndex {
                base: argument,
                index: member_index,
            } = function.expressions[base]
            else {
                return false;
            };
            let Expression::FunctionArgument(position) = function.expressions[argument] else {
                return false;
            };
            let ty = function.arguments[position as usize].ty;
            let TypeInner::Struct { members, .. } = &self.module.types[ty].inner else {
                return false;
            };
            members
                .get(member_index as usize)
                .and_then(|m| m.name.as_deref())
                == Some(member)
        })
    }

    /// Returns the `u32` literals that `global.member` is compared against in `if` conditions.
    pub fn branch_values_of_member(
        &self,
        function: &Function,
        global_name: &str,
        member: &str,
    ) -> BTreeSet<u32> {
        let mut values = BTreeSet::new();
        visit_statements(&function.body, &mut |statement| {
            if let Statement::If { condition, .. } = statement {
                if let Some(value) =
                    self.member_equals_literal(function, *condition, global_name, member)
                {
                    values.insert(value);
                }
            }
        });
        values
    }

    /// Decodes the condition `global.member == <u32 literal>`.
    fn member_equals_literal(
        &self,
        function: &Function,
        condition: Handle<Expression>,
        global_name: &str,
        member: &str,
    ) -> Option<u32> {
        let Expression::Binary {
            op: naga::BinaryOperator::Equal,
            left,
            right,
        } = function.expressions[condition]
        else {
            return None;
        };
        let Expression::Literal(naga::Literal::U32(value)) = function.expressions[right] else {
            return None;
        };
        let Expression::Load { pointer } = function.expressions[left] else {
            return None;
        };
        let Expression::AccessIndex { base, index } = function.expressions[pointer] else {
            return None;
        };
        let Expression::GlobalVariable(global) = function.expressions[base] else {
            return None;
        };
        let variable = &self.module.global_variables[global];
        let TypeInner::Struct { members, .. } = &self.module.types[variable.ty].inner else {
            return None;
        };
        (variable.name.as_deref() == Some(global_name)
            && members.get(index as usize)?.name.as_deref() == Some(member))
        .then_some(value)
    }
}

/// Returns `true` when `function` has a local `let` called `name`.
pub fn has_named_let(function: &Function, name: &str) -> bool {
    function
        .named_expressions
        .values()
        .any(|named| named == name)
}

/// Returns `true` when `function` reaches a `discard` in any nested block.
pub fn discards(function: &Function) -> bool {
    let mut found = false;
    visit_statements(&function.body, &mut |statement| {
        found |= matches!(statement, Statement::Kill);
    });
    found
}

/// Returns `true` when `function` runs `discard` under `if (<name> < <threshold>)`.
pub fn discards_when_below(function: &Function, name: &str, threshold: f32) -> bool {
    let mut found = false;
    visit_statements(&function.body, &mut |statement| {
        let Statement::If {
            condition, accept, ..
        } = statement
        else {
            return;
        };
        let Expression::Binary {
            op: naga::BinaryOperator::Less,
            left,
            right,
        } = function.expressions[*condition]
        else {
            return;
        };
        let compares_name = function.named_expressions.get(&left).map(String::as_str) == Some(name);
        let compares_threshold = matches!(
            function.expressions[right],
            Expression::Literal(naga::Literal::F32(value)) if (value - threshold).abs() < f32::EPSILON
        );
        let accept_discards = accept.iter().any(|s| matches!(s, Statement::Kill));
        found |= compares_name && compares_threshold && accept_discards;
    });
    found
}

/// Calls `visit` for every statement in `block`, descending into all nested control flow.
fn visit_statements(block: &Block, visit: &mut impl FnMut(&Statement)) {
    for statement in block {
        visit(statement);
        match statement {
            Statement::Block(inner) => visit_statements(inner, visit),
            Statement::If { accept, reject, .. } => {
                visit_statements(accept, visit);
                visit_statements(reject, visit);
            }
            Statement::Switch { cases, .. } => {
                for case in cases {
                    visit_statements(&case.body, visit);
                }
            }
            Statement::Loop {
                body, continuing, ..
            } => {
                visit_statements(body, visit);
                visit_statements(continuing, visit);
            }
            _ => {}
        }
    }
}
