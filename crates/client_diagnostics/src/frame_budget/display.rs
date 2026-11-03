//! The display path a frame-budget run presents through, declared and checked.
//!
//! # Responsibility
//! Owns [`DisplayPath`], the `--display-path` value of the `frame_budget` tool, and
//! [`check_display_path`], which compares that declaration with the process environment before
//! anything else of the run happens. The plan `docs/plans/2026-11-03-close-phase-6-7-8-exits.md`
//! (decision D-4, "Display path", and work package `WP5b`) explains why: on the development server
//! Xvfb can present a GPU image only through Mesa's software path, which copies every frame
//! through the CPU and costs about 100 ms per 1280×720 frame (§5.3 of that plan). A report from
//! that path and one from a hardware path must never look alike.
//!
//! # How it fits
//! [`super::run`] calls [`check_display_path`] first, with the real environment as its lookup,
//! and records the declared path in the report ([`super::report::RunEnvironment::display_path`]).
//! The check is a pure function of its lookup, so the unit tests below drive it with tables.
//!
//! # Invariants
//! - A variable that is set to the empty string counts as unset.
//! - `MESA_VK_WSI_DEBUG` is a comma-separated list of flags; the software present path is the
//!   flag `sw`.
//! - The check cannot prove that a Wayland server is a headless `KWin` rather than a desktop. It
//!   refuses the mistakes that matter: a software present labelled as hardware, a Wayland session
//!   labelled as Xvfb, and a missing display. Every refusal names the variable at fault.

use serde::{Deserialize, Serialize};

/// The X11 display variable.
pub const DISPLAY: &str = "DISPLAY";
/// The Wayland display variable.
pub const WAYLAND_DISPLAY: &str = "WAYLAND_DISPLAY";
/// Mesa's Vulkan window-system debug flags; `sw` selects software presentation.
pub const MESA_VK_WSI_DEBUG: &str = "MESA_VK_WSI_DEBUG";

/// Where a run's frames are presented.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, clap::ValueEnum,
)]
pub enum DisplayPath {
    /// A headless `KWin` compositor (`kwin_wayland --virtual --xwayland`): the client presents to
    /// `KWin`'s Xwayland over DRI3 with the driver's normal presentation, and `KWin` composites on
    /// the same GPU. The path of record on the development server.
    KwinVirtual,
    /// Xvfb with software presentation: radv with `MESA_VK_WSI_DEBUG=sw`, or `lavapipe` (CI),
    /// where every presented frame is a CPU copy.
    XvfbSoftwarePresent,
    /// A desktop session on a real monitor, X11 or Wayland, with hardware presentation.
    Desktop,
}

impl DisplayPath {
    /// The value the command line takes for this path.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::KwinVirtual => "kwin-virtual",
            Self::XvfbSoftwarePresent => "xvfb-software-present",
            Self::Desktop => "desktop",
        }
    }

    /// One line, for the report, on what this path measures.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::KwinVirtual => {
                "headless KWin compositor through its Xwayland: hardware present, frames stay on \
                 the GPU and KWin composites them on the same GPU; no monitor"
            }
            Self::XvfbSoftwarePresent => {
                "Xvfb with software present: every frame is copied through the CPU into the X \
                 server, so the frame interval measures the display path, not the game"
            }
            Self::Desktop => "desktop session on a real monitor, hardware present",
        }
    }
}

impl std::fmt::Display for DisplayPath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.name())
    }
}

/// Why the declared display path contradicts the environment.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DisplayPathError {
    /// A variable the path needs is unset.
    #[error("--display-path {path} needs {variable} to be set, and it is not")]
    Missing {
        /// The declared path.
        path: DisplayPath,
        /// The unset variable.
        variable: &'static str,
    },
    /// A variable that the path rules out is set.
    #[error("--display-path {path} rules out {variable}, which is set to {value:?}")]
    Contradicted {
        /// The declared path.
        path: DisplayPath,
        /// The variable that is set.
        variable: &'static str,
        /// Its value.
        value: String,
    },
    /// Neither display variable is set.
    #[error(
        "--display-path {path} needs {DISPLAY} or {WAYLAND_DISPLAY} to be set, and neither is"
    )]
    NoDisplay {
        /// The declared path.
        path: DisplayPath,
    },
}

impl DisplayPathError {
    /// The variable at fault; for [`Self::NoDisplay`], [`DISPLAY`].
    #[must_use]
    pub fn variable(&self) -> &'static str {
        match self {
            Self::Missing { variable, .. } | Self::Contradicted { variable, .. } => variable,
            Self::NoDisplay { .. } => DISPLAY,
        }
    }
}

/// Checks the declared `path` against the environment `lookup` gives.
///
/// `lookup` returns a variable's value, or `None` when it is unset; an empty value counts as
/// unset. The rules:
/// - [`DisplayPath::KwinVirtual`]: `WAYLAND_DISPLAY` and `DISPLAY` are set (the client reaches
///   `KWin` through its Xwayland), and `MESA_VK_WSI_DEBUG` has no `sw` flag;
/// - [`DisplayPath::XvfbSoftwarePresent`]: `DISPLAY` is set and `WAYLAND_DISPLAY` is not;
/// - [`DisplayPath::Desktop`]: `DISPLAY` or `WAYLAND_DISPLAY` is set, and `MESA_VK_WSI_DEBUG` has
///   no `sw` flag.
///
/// # Errors
/// The first broken rule, as a [`DisplayPathError`] naming its variable.
pub fn check_display_path(
    path: DisplayPath,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<(), DisplayPathError> {
    let get = |variable: &str| lookup(variable).filter(|value| !value.is_empty());
    let require = |variable: &'static str| {
        get(variable)
            .map(|_| ())
            .ok_or(DisplayPathError::Missing { path, variable })
    };
    let forbid_software_present = || match get(MESA_VK_WSI_DEBUG) {
        Some(value) if value.split(',').any(|flag| flag.trim() == "sw") => {
            Err(DisplayPathError::Contradicted {
                path,
                variable: MESA_VK_WSI_DEBUG,
                value,
            })
        }
        _ => Ok(()),
    };
    match path {
        DisplayPath::KwinVirtual => {
            require(WAYLAND_DISPLAY)?;
            require(DISPLAY)?;
            forbid_software_present()
        }
        DisplayPath::XvfbSoftwarePresent => {
            require(DISPLAY)?;
            match get(WAYLAND_DISPLAY) {
                Some(value) => Err(DisplayPathError::Contradicted {
                    path,
                    variable: WAYLAND_DISPLAY,
                    value,
                }),
                None => Ok(()),
            }
        }
        DisplayPath::Desktop => {
            if get(DISPLAY).is_none() && get(WAYLAND_DISPLAY).is_none() {
                return Err(DisplayPathError::NoDisplay { path });
            }
            forbid_software_present()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::ValueEnum;

    /// One row of the table: path, environment, and `None` to accept or the variable at fault.
    type Case = (
        DisplayPath,
        &'static [(&'static str, &'static str)],
        Option<&'static str>,
    );

    const KWIN: &[(&str, &str)] = &[(WAYLAND_DISPLAY, "frame-budget-01"), (DISPLAY, ":1")];
    const KWIN_SW: &[(&str, &str)] = &[
        (WAYLAND_DISPLAY, "frame-budget-01"),
        (DISPLAY, ":1"),
        (MESA_VK_WSI_DEBUG, "sw"),
    ];
    const X_ONLY: &[(&str, &str)] = &[(DISPLAY, ":99")];
    const X_SW: &[(&str, &str)] = &[(DISPLAY, ":99"), (MESA_VK_WSI_DEBUG, "sw")];
    const X_SW_LIST: &[(&str, &str)] = &[(DISPLAY, ":99"), (MESA_VK_WSI_DEBUG, "nofence,sw")];
    const WAYLAND_ONLY: &[(&str, &str)] = &[(WAYLAND_DISPLAY, "wayland-0")];
    const NONE: &[(&str, &str)] = &[];
    const EMPTY_WAYLAND: &[(&str, &str)] = &[(WAYLAND_DISPLAY, ""), (DISPLAY, ":1")];

    const CASES: &[Case] = &[
        (DisplayPath::KwinVirtual, KWIN, None),
        (DisplayPath::KwinVirtual, KWIN_SW, Some(MESA_VK_WSI_DEBUG)),
        (DisplayPath::KwinVirtual, X_ONLY, Some(WAYLAND_DISPLAY)),
        (
            DisplayPath::KwinVirtual,
            EMPTY_WAYLAND,
            Some(WAYLAND_DISPLAY),
        ),
        (DisplayPath::KwinVirtual, WAYLAND_ONLY, Some(DISPLAY)),
        (DisplayPath::XvfbSoftwarePresent, X_ONLY, None),
        (DisplayPath::XvfbSoftwarePresent, X_SW, None),
        (
            DisplayPath::XvfbSoftwarePresent,
            KWIN,
            Some(WAYLAND_DISPLAY),
        ),
        (DisplayPath::XvfbSoftwarePresent, NONE, Some(DISPLAY)),
        (DisplayPath::Desktop, X_ONLY, None),
        (DisplayPath::Desktop, WAYLAND_ONLY, None),
        (DisplayPath::Desktop, KWIN, None),
        (DisplayPath::Desktop, NONE, Some(DISPLAY)),
        (DisplayPath::Desktop, X_SW, Some(MESA_VK_WSI_DEBUG)),
        (DisplayPath::Desktop, X_SW_LIST, Some(MESA_VK_WSI_DEBUG)),
    ];

    fn lookup(
        environment: &'static [(&'static str, &'static str)],
    ) -> impl Fn(&str) -> Option<String> {
        move |name| {
            environment
                .iter()
                .find(|(variable, _)| *variable == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn each_path_accepts_its_environment_and_names_the_variable_it_refuses() {
        for &(path, environment, refused) in CASES {
            let result = check_display_path(path, lookup(environment));
            let error = result.as_ref().err();
            assert_eq!(
                error.map(DisplayPathError::variable),
                refused,
                "{path} in {environment:?}: {result:?}"
            );
            if let (Some(variable), Some(error)) = (refused, error) {
                assert!(
                    error.to_string().contains(variable),
                    "{path} in {environment:?}: {error}"
                );
            }
        }
    }

    #[test]
    fn the_command_line_names_match_the_report_names() {
        for path in DisplayPath::value_variants() {
            let value = path.to_possible_value();
            assert_eq!(
                value.as_ref().map(clap::builder::PossibleValue::get_name),
                Some(path.name())
            );
            assert!(path.description().len() > path.name().len());
        }
    }
}
