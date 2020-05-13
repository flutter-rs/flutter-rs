#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum FlutterPointerPhase {
    Cancel,
    Up,
    Down,
    Move,
    Add,
    Remove,
    Hover,
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
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FlutterPointerDeviceKind {
    Mouse,
    Touch,
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
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum FlutterPointerSignalKind {
    None,
    Scroll,
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
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum FlutterPointerMouseButtons {
    Primary,
    Secondary,
    Middle,
    Back,
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
