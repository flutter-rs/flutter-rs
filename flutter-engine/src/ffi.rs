//! Embedder enums re-exported in embedder-friendly form.
//!
//! These mirror the `flutter_engine_sys` enums but are plain Rust enums the
//! embedder constructs itself.
//!
//! The three pointer enums are `#[non_exhaustive]` because the embedder API
//! keeps growing variants — 3.47 added `kFlutterPointerDeviceKindInvertedStylus`
//! — and the attribute only forbids an exhaustive `match` in a downstream
//! crate: every variant stays constructible, so the next addition is not a
//! breaking change. [`KeyEventType`] and [`FlutterPointerMouseButtons`] are
//! left exhaustive: both are complete, closed sets in the embedder header, and
//! a sixth mouse button is expressed through [`PointerButtons::from_bits`]
//! rather than a new variant.

/// The kind of a key transition, as reported to `HardwareKeyboard`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum KeyEventType {
    /// The key was released.
    Up,
    /// The key was pressed.
    Down,
    /// The key auto-repeated while held.
    Repeat,
}

impl From<KeyEventType> for flutter_engine_sys::FlutterKeyEventType {
    fn from(value: KeyEventType) -> Self {
        match value {
            KeyEventType::Up => flutter_engine_sys::FlutterKeyEventType::kFlutterKeyEventTypeUp,
            KeyEventType::Down => flutter_engine_sys::FlutterKeyEventType::kFlutterKeyEventTypeDown,
            KeyEventType::Repeat => {
                flutter_engine_sys::FlutterKeyEventType::kFlutterKeyEventTypeRepeat
            }
        }
    }
}

/// What a pointer did, as reported to the framework's gesture arena.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FlutterPointerPhase {
    /// The pointer's sequence was aborted without a release.
    Cancel,
    /// The pointer was released.
    Up,
    /// The pointer was pressed.
    Down,
    /// The pointer moved while pressed.
    Move,
    /// A new pointer appeared, not yet pressed.
    Add,
    /// A pointer disappeared.
    Remove,
    /// The pointer moved while not pressed.
    Hover,
    /// A trackpad pan/zoom gesture started.
    PanZoomStart,
    /// A trackpad pan/zoom gesture moved; the event's
    /// [`PanZoom`] carries the pan, scale and rotation.
    PanZoomUpdate,
    /// A trackpad pan/zoom gesture ended.
    PanZoomEnd,
}

impl From<FlutterPointerPhase> for flutter_engine_sys::FlutterPointerPhase {
    fn from(pointer_phase: FlutterPointerPhase) -> Self {
        match pointer_phase {
            FlutterPointerPhase::Cancel => flutter_engine_sys::FlutterPointerPhase::kCancel,
            FlutterPointerPhase::Up => flutter_engine_sys::FlutterPointerPhase::kUp,
            FlutterPointerPhase::Down => flutter_engine_sys::FlutterPointerPhase::kDown,
            FlutterPointerPhase::Move => flutter_engine_sys::FlutterPointerPhase::kMove,
            FlutterPointerPhase::Add => flutter_engine_sys::FlutterPointerPhase::kAdd,
            FlutterPointerPhase::Remove => flutter_engine_sys::FlutterPointerPhase::kRemove,
            FlutterPointerPhase::Hover => flutter_engine_sys::FlutterPointerPhase::kHover,
            FlutterPointerPhase::PanZoomStart => {
                flutter_engine_sys::FlutterPointerPhase::kPanZoomStart
            }
            FlutterPointerPhase::PanZoomUpdate => {
                flutter_engine_sys::FlutterPointerPhase::kPanZoomUpdate
            }
            FlutterPointerPhase::PanZoomEnd => flutter_engine_sys::FlutterPointerPhase::kPanZoomEnd,
        }
    }
}

/// The hardware behind a pointer.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum FlutterPointerDeviceKind {
    /// A mouse. Reporting this obliges the embedder to also report the
    /// currently pressed [`PointerButtons`] on every event.
    Mouse,
    /// A touch screen.
    Touch,
    /// A stylus, tip down.
    Stylus,
    /// A stylus held the other way up, i.e. used as an eraser.
    InvertedStylus,
    /// A trackpad, which also produces the `PanZoom*` phases.
    Trackpad,
}

impl From<FlutterPointerDeviceKind> for flutter_engine_sys::FlutterPointerDeviceKind {
    fn from(device_kind: FlutterPointerDeviceKind) -> Self {
        match device_kind {
            FlutterPointerDeviceKind::Mouse => {
                flutter_engine_sys::FlutterPointerDeviceKind::kFlutterPointerDeviceKindMouse
            }
            FlutterPointerDeviceKind::Touch => {
                flutter_engine_sys::FlutterPointerDeviceKind::kFlutterPointerDeviceKindTouch
            }
            FlutterPointerDeviceKind::Stylus => {
                flutter_engine_sys::FlutterPointerDeviceKind::kFlutterPointerDeviceKindStylus
            }
            FlutterPointerDeviceKind::InvertedStylus => {
                flutter_engine_sys::FlutterPointerDeviceKind::kFlutterPointerDeviceKindInvertedStylus
            }
            FlutterPointerDeviceKind::Trackpad => {
                flutter_engine_sys::FlutterPointerDeviceKind::kFlutterPointerDeviceKindTrackpad
            }
        }
    }
}

/// A non-positional signal carried by a pointer event.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FlutterPointerSignalKind {
    /// No signal; an ordinary pointer event.
    None,
    /// A scroll, whose delta is in the event's scroll fields.
    Scroll,
    /// The momentum of a previous scroll was cancelled, e.g. because the
    /// user put two fingers back on the trackpad.
    ScrollInertiaCancel,
    /// A zoom, whose factor is in the event's [`PanZoom`].
    Scale,
}

impl From<FlutterPointerSignalKind> for flutter_engine_sys::FlutterPointerSignalKind {
    fn from(pointer_signal_kind: FlutterPointerSignalKind) -> Self {
        match pointer_signal_kind {
            FlutterPointerSignalKind::None => {
                flutter_engine_sys::FlutterPointerSignalKind::kFlutterPointerSignalKindNone
            }
            FlutterPointerSignalKind::Scroll => {
                flutter_engine_sys::FlutterPointerSignalKind::kFlutterPointerSignalKindScroll
            }
            FlutterPointerSignalKind::ScrollInertiaCancel => {
                flutter_engine_sys::FlutterPointerSignalKind::kFlutterPointerSignalKindScrollInertiaCancel
            }
            FlutterPointerSignalKind::Scale => {
                flutter_engine_sys::FlutterPointerSignalKind::kFlutterPointerSignalKindScale
            }
        }
    }
}

/// The pan, scale and rotation of a trackpad gesture, carried by the
/// `PanZoom*` phases of [`FlutterPointerPhase`] and by
/// [`FlutterPointerSignalKind::Scale`].
///
/// [`PanZoom::default`] is the neutral state the engine expects on an event
/// that is not a pan/zoom: no pan, scale 1.0, no rotation.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct PanZoom {
    /// The pan offset in physical pixels.
    pub pan: (f64, f64),
    /// The zoom factor, where 1.0 is the scale the gesture started at.
    pub scale: f64,
    /// The rotation in radians, where 0.0 is the angle the gesture started at.
    pub rotation: f64,
}

impl Default for PanZoom {
    fn default() -> Self {
        Self {
            pan: (0.0, 0.0),
            scale: 1.0,
            rotation: 0.0,
        }
    }
}

/// Which mouse button a pointer event refers to.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum FlutterPointerMouseButtons {
    /// Usually the left button.
    Primary,
    /// Usually the right button.
    Secondary,
    /// The wheel button.
    Middle,
    /// The "back" side button.
    Back,
    /// The "forward" side button.
    Forward,
}

impl From<FlutterPointerMouseButtons> for flutter_engine_sys::FlutterPointerMouseButtons {
    fn from(btn: FlutterPointerMouseButtons) -> Self {
        match btn {
            FlutterPointerMouseButtons::Primary => {
                flutter_engine_sys::FlutterPointerMouseButtons::kFlutterPointerButtonMousePrimary
            }
            FlutterPointerMouseButtons::Secondary => {
                flutter_engine_sys::FlutterPointerMouseButtons::kFlutterPointerButtonMouseSecondary
            }
            FlutterPointerMouseButtons::Middle => {
                flutter_engine_sys::FlutterPointerMouseButtons::kFlutterPointerButtonMouseMiddle
            }
            FlutterPointerMouseButtons::Back => {
                flutter_engine_sys::FlutterPointerMouseButtons::kFlutterPointerButtonMouseBack
            }
            FlutterPointerMouseButtons::Forward => {
                flutter_engine_sys::FlutterPointerMouseButtons::kFlutterPointerButtonMouseForward
            }
        }
    }
}

/// The set of pointer buttons currently held down.
///
/// `FlutterPointerEvent::buttons` is an `int64_t` bit mask, not a single
/// button: an embedder that reports [`FlutterPointerDeviceKind::Mouse`] must
/// send the *cumulative* mask on every event, including [`Self::NONE`] while
/// the pointer merely hovers. The mask is read as
/// `FlutterPointerMouseButtons` for a mouse and as `FlutterPointerStylusButtons`
/// for a stylus, which is why the stylus flags alias the first three mouse
/// flags.
///
/// The example is a `text` block rather than a doctest because a doctest has
/// to link `libflutter_engine`, and rustdoc gets the link search path only
/// from `flutter-engine-sys`'s build script, i.e. only when
/// `FLUTTER_ENGINE_LIB_DIR` is set — `RUSTFLAGS="-L …"` does not reach
/// rustdoc. The three unit tests below assert the same things unconditionally.
///
/// ```text
/// use flutter_engine::ffi::{FlutterPointerMouseButtons, PointerButtons};
///
/// let dragging = PointerButtons::PRIMARY | PointerButtons::MIDDLE;
/// assert!(dragging.contains(PointerButtons::PRIMARY));
/// assert_eq!(dragging.bits(), 0b101);
/// assert!(PointerButtons::NONE.is_empty());
/// assert_eq!(
///     PointerButtons::from(FlutterPointerMouseButtons::Secondary),
///     PointerButtons::SECONDARY
/// );
/// ```
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct PointerButtons(i64);

impl PointerButtons {
    // The flag values are the embedder header's `1 << n`, written out because
    // `1 << 0` is an identity shift clippy rejects.
    /// No button is pressed.
    pub const NONE: Self = Self(0);
    /// Usually the left mouse button.
    pub const PRIMARY: Self = Self(1);
    /// Usually the right mouse button.
    pub const SECONDARY: Self = Self(1 << 1);
    /// The mouse wheel button.
    pub const MIDDLE: Self = Self(1 << 2);
    /// The "back" side mouse button.
    pub const BACK: Self = Self(1 << 3);
    /// The "forward" side mouse button.
    pub const FORWARD: Self = Self(1 << 4);
    /// The stylus is touching the screen. Aliases [`Self::PRIMARY`]; the
    /// engine tells them apart by the event's device kind.
    pub const STYLUS_CONTACT: Self = Self(1);
    /// The stylus's primary button. Aliases [`Self::SECONDARY`].
    pub const STYLUS_PRIMARY: Self = Self(1 << 1);
    /// The stylus's secondary button. Aliases [`Self::MIDDLE`].
    pub const STYLUS_SECONDARY: Self = Self(1 << 2);

    /// The mask the engine receives.
    #[must_use]
    pub const fn bits(self) -> i64 {
        self.0
    }

    /// Build a mask from raw embedder flags.
    ///
    /// The header reserves `1 << 5` and up for the sixth and further buttons
    /// of a mouse that has them, which have no named constant here.
    #[must_use]
    pub const fn from_bits(bits: i64) -> Self {
        Self(bits)
    }

    /// Whether no button at all is pressed.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether every button in `other` is pressed. Always true for
    /// [`Self::NONE`].
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl From<FlutterPointerMouseButtons> for PointerButtons {
    fn from(button: FlutterPointerMouseButtons) -> Self {
        match button {
            FlutterPointerMouseButtons::Primary => Self::PRIMARY,
            FlutterPointerMouseButtons::Secondary => Self::SECONDARY,
            FlutterPointerMouseButtons::Middle => Self::MIDDLE,
            FlutterPointerMouseButtons::Back => Self::BACK,
            FlutterPointerMouseButtons::Forward => Self::FORWARD,
        }
    }
}

impl std::ops::BitOr for PointerButtons {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for PointerButtons {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A copy-paste slip in one of these `match` arms would silently send the
    /// engine the wrong phase or button, so check every variant maps to a
    /// distinct embedder value.
    #[test]
    fn every_variant_maps_to_a_distinct_embedder_value() {
        fn all_distinct<T: Eq + std::fmt::Debug>(values: &[T]) {
            for (index, value) in values.iter().enumerate() {
                assert!(
                    !values[index + 1..].contains(value),
                    "{value:?} is produced by more than one variant"
                );
            }
        }

        all_distinct(
            &[KeyEventType::Up, KeyEventType::Down, KeyEventType::Repeat]
                .map(flutter_engine_sys::FlutterKeyEventType::from),
        );
        all_distinct(
            &[
                FlutterPointerPhase::Cancel,
                FlutterPointerPhase::Up,
                FlutterPointerPhase::Down,
                FlutterPointerPhase::Move,
                FlutterPointerPhase::Add,
                FlutterPointerPhase::Remove,
                FlutterPointerPhase::Hover,
                FlutterPointerPhase::PanZoomStart,
                FlutterPointerPhase::PanZoomUpdate,
                FlutterPointerPhase::PanZoomEnd,
            ]
            .map(flutter_engine_sys::FlutterPointerPhase::from),
        );
        all_distinct(
            &[
                FlutterPointerDeviceKind::Mouse,
                FlutterPointerDeviceKind::Touch,
                FlutterPointerDeviceKind::Stylus,
                FlutterPointerDeviceKind::InvertedStylus,
                FlutterPointerDeviceKind::Trackpad,
            ]
            .map(flutter_engine_sys::FlutterPointerDeviceKind::from),
        );
        all_distinct(
            &[
                FlutterPointerSignalKind::None,
                FlutterPointerSignalKind::Scroll,
                FlutterPointerSignalKind::ScrollInertiaCancel,
                FlutterPointerSignalKind::Scale,
            ]
            .map(flutter_engine_sys::FlutterPointerSignalKind::from),
        );
        all_distinct(
            &[
                FlutterPointerMouseButtons::Primary,
                FlutterPointerMouseButtons::Secondary,
                FlutterPointerMouseButtons::Middle,
                FlutterPointerMouseButtons::Back,
                FlutterPointerMouseButtons::Forward,
            ]
            .map(flutter_engine_sys::FlutterPointerMouseButtons::from),
        );
    }

    /// The button values are a bit mask the engine ORs together, and
    /// `send_pointer_event` widens them to `i64`.
    #[test]
    fn mouse_buttons_are_the_embedder_bit_mask() {
        assert_eq!(
            flutter_engine_sys::FlutterPointerMouseButtons::from(
                FlutterPointerMouseButtons::Primary
            ) as i64,
            1
        );
        assert_eq!(
            flutter_engine_sys::FlutterPointerMouseButtons::from(
                FlutterPointerMouseButtons::Secondary
            ) as i64,
            2
        );
        assert_eq!(
            flutter_engine_sys::FlutterPointerMouseButtons::from(FlutterPointerMouseButtons::Middle)
                as i64,
            4
        );
    }

    /// `PointerButtons` is what actually reaches `FlutterPointerEvent::buttons`,
    /// so every flag has to equal the embedder enum it stands for.
    #[test]
    fn pointer_buttons_flags_match_the_embedder_enum() {
        for (button, flag) in [
            (FlutterPointerMouseButtons::Primary, PointerButtons::PRIMARY),
            (
                FlutterPointerMouseButtons::Secondary,
                PointerButtons::SECONDARY,
            ),
            (FlutterPointerMouseButtons::Middle, PointerButtons::MIDDLE),
            (FlutterPointerMouseButtons::Back, PointerButtons::BACK),
            (FlutterPointerMouseButtons::Forward, PointerButtons::FORWARD),
        ] {
            assert_eq!(PointerButtons::from(button), flag, "{button:?}");
            assert_eq!(
                flag.bits(),
                flutter_engine_sys::FlutterPointerMouseButtons::from(button) as i64,
                "{button:?}"
            );
        }
        // The stylus flags share the wire bits of the first three mouse flags.
        assert_eq!(PointerButtons::STYLUS_CONTACT, PointerButtons::PRIMARY);
        assert_eq!(PointerButtons::STYLUS_PRIMARY, PointerButtons::SECONDARY);
        assert_eq!(PointerButtons::STYLUS_SECONDARY, PointerButtons::MIDDLE);
        // The sixth button and up have no constant; the header says 1 << 5.
        assert_eq!(PointerButtons::from_bits(1 << 5).bits(), 32);
    }

    /// The whole point of the newtype: "no button" and "two buttons" must both
    /// be expressible, which a single `FlutterPointerMouseButtons` cannot do.
    #[test]
    fn pointer_buttons_compose_into_a_mask() {
        assert_eq!(PointerButtons::NONE.bits(), 0);
        assert_eq!(PointerButtons::default(), PointerButtons::NONE);
        assert!(PointerButtons::NONE.is_empty());

        let mut held = PointerButtons::PRIMARY | PointerButtons::SECONDARY;
        assert_eq!(held.bits(), 0b11);
        assert!(!held.is_empty());
        assert!(held.contains(PointerButtons::PRIMARY));
        assert!(held.contains(PointerButtons::SECONDARY));
        assert!(!held.contains(PointerButtons::MIDDLE));
        assert!(!held.contains(PointerButtons::PRIMARY | PointerButtons::MIDDLE));
        // `contains` of the empty set is vacuously true.
        assert!(held.contains(PointerButtons::NONE));
        assert!(PointerButtons::NONE.contains(PointerButtons::NONE));

        held |= PointerButtons::MIDDLE;
        assert_eq!(held.bits(), 0b111);
        // Or-ing a flag that is already set is a no-op.
        held |= PointerButtons::MIDDLE;
        assert_eq!(held.bits(), 0b111);
    }

    /// The engine treats `scale` as "1.0 is the initial scale", so a neutral
    /// pan/zoom must not be all-zero.
    #[test]
    fn pan_zoom_default_is_neutral() {
        let neutral = PanZoom::default();
        assert_eq!(neutral.pan, (0.0, 0.0));
        assert_eq!(neutral.scale, 1.0);
        assert_eq!(neutral.rotation, 0.0);
    }
}
