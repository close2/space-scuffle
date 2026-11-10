//! Bindings: the sizes and timings an event publishes for its effect (plan §5.4).
//!
//! # Responsibility
//! Owns [`BoundValues`] (the values of one live effect, by [`Binding`]), which event kinds
//! provide which bindings ([`provides`]), and how a [`ValueDef`] turns into a number
//! ([`resolve_value`]).
//!
//! # How it fits
//! The catalog resolver refuses a layer binding a value its effect's event kind does not
//! provide; the event wiring fills [`BoundValues`] from the event once at spawn; the runtime
//! reads them every frame.
//!
//! # Invariants
//! - [`Binding::Const`] is always 1 and always provided.
//! - A binding not filled by the event reads 0.

use super::catalog::schema::{Binding, EventKind, ValueDef};

/// Number of [`Binding`] variants.
pub const BINDING_COUNT: usize = 15;

/// Dense index of a binding.
#[must_use]
pub const fn binding_index(binding: Binding) -> usize {
    match binding {
        Binding::Const => 0,
        Binding::SplashFull => 1,
        Binding::SplashMid => 2,
        Binding::SplashReach => 3,
        Binding::LineLength => 4,
        Binding::LineSpacing => 5,
        Binding::AreaRadius => 6,
        Binding::AreaRemaining => 7,
        Binding::AreaTotal => 8,
        Binding::LinkLength => 9,
        Binding::UnitRadius => 10,
        Binding::UnitHeight => 11,
        Binding::ChannelProgress => 12,
        Binding::BarrierRatio => 13,
        Binding::FlightTime => 14,
    }
}

/// Whether events of kind `event` publish `binding`.
#[must_use]
pub const fn provides(event: EventKind, binding: Binding) -> bool {
    use Binding as B;
    match binding {
        B::Const => true,
        B::SplashFull | B::SplashMid | B::SplashReach => {
            matches!(event, EventKind::Splash | EventKind::Line)
        }
        B::LineLength | B::LineSpacing => matches!(event, EventKind::Line),
        B::AreaRadius | B::AreaRemaining | B::AreaTotal => matches!(event, EventKind::Area),
        B::LinkLength => matches!(event, EventKind::Link),
        B::UnitRadius | B::UnitHeight => matches!(
            event,
            EventKind::Status | EventKind::Mode | EventKind::Death
        ),
        B::ChannelProgress => matches!(event, EventKind::Mode),
        B::BarrierRatio => matches!(event, EventKind::Status),
        B::FlightTime => matches!(event, EventKind::Point),
    }
}

/// The bound values of one live effect.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundValues {
    /// Values by [`binding_index`]; index 0 is the constant 1.
    values: [f32; BINDING_COUNT],
}

impl Default for BoundValues {
    fn default() -> Self {
        let mut values = [0.0; BINDING_COUNT];
        values[0] = 1.0;
        Self { values }
    }
}

impl BoundValues {
    /// The value of `binding`.
    #[must_use]
    pub const fn get(&self, binding: Binding) -> f32 {
        self.values[binding_index(binding)]
    }

    /// Sets `binding` to `value`; setting [`Binding::Const`] is ignored.
    pub fn set(&mut self, binding: Binding, value: f32) {
        if binding != Binding::Const {
            self.values[binding_index(binding)] = value;
        }
    }

    /// Builder form of [`Self::set`].
    #[must_use]
    pub fn with(mut self, binding: Binding, value: f32) -> Self {
        self.set(binding, value);
        self
    }
}

/// The number `value` stands for under `bound`: the bound value times its scale.
#[must_use]
pub fn resolve_value(value: &ValueDef, bound: &BoundValues) -> f32 {
    bound.get(value.bind) * value.scale
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Binding; BINDING_COUNT] = [
        Binding::Const,
        Binding::SplashFull,
        Binding::SplashMid,
        Binding::SplashReach,
        Binding::LineLength,
        Binding::LineSpacing,
        Binding::AreaRadius,
        Binding::AreaRemaining,
        Binding::AreaTotal,
        Binding::LinkLength,
        Binding::UnitRadius,
        Binding::UnitHeight,
        Binding::ChannelProgress,
        Binding::BarrierRatio,
        Binding::FlightTime,
    ];

    #[test]
    fn indices_are_dense_and_distinct() {
        for (expected, binding) in ALL.iter().enumerate() {
            assert_eq!(binding_index(*binding), expected);
        }
    }

    #[test]
    fn a_splash_provides_its_zones_but_no_line_or_unit_value() {
        assert!(provides(EventKind::Splash, Binding::SplashReach));
        assert!(provides(EventKind::Splash, Binding::Const));
        assert!(!provides(EventKind::Splash, Binding::LineLength));
        assert!(!provides(EventKind::Death, Binding::SplashReach));
        assert!(provides(EventKind::Line, Binding::LineSpacing));
        assert!(provides(EventKind::Line, Binding::SplashFull));
        assert!(provides(EventKind::Death, Binding::UnitRadius));
    }

    #[test]
    fn a_value_is_its_binding_times_its_scale() {
        let bound = BoundValues::default().with(Binding::SplashReach, 1.25);
        let reach = ValueDef {
            bind: Binding::SplashReach,
            scale: 1.6,
        };
        assert!((resolve_value(&reach, &bound) - 2.0).abs() < 1e-6);
        let constant = ValueDef {
            bind: Binding::Const,
            scale: 0.5,
        };
        assert!((resolve_value(&constant, &bound) - 0.5).abs() < 1e-6);
        let unfilled = ValueDef {
            bind: Binding::SplashFull,
            scale: 3.0,
        };
        assert!(
            resolve_value(&unfilled, &bound).abs() < 1e-6,
            "unfilled reads 0"
        );
        assert!((bound.with(Binding::Const, 9.0).get(Binding::Const) - 1.0).abs() < 1e-6);
    }
}
