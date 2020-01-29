use flutter_engine::ffi::{
    FlutterPointerDeviceKind, FlutterPointerMouseButtons, FlutterPointerPhase,
    FlutterPointerSignalKind,
};
use flutter_engine::FlutterEngine;
use glutin::event::{DeviceId, ElementState, MouseButton, TouchPhase};

pub struct Pointer {
    device_id: DeviceId,
    touch: bool,
    position: (f64, f64),
    pressed: u32,
}

impl Pointer {
    pub fn new(device_id: DeviceId, touch: bool) -> Self {
        Self {
            device_id,
            touch,
            position: (0.0, 0.0),
            pressed: 0,
        }
    }
}

pub struct Pointers {
    engine: FlutterEngine,
    pointers: Vec<Pointer>,
}

impl Pointers {
    pub fn new(engine: FlutterEngine) -> Self {
        Self {
            engine,
            pointers: Default::default(),
        }
    }

    pub fn index(&mut self, device_id: DeviceId, touch: bool) -> usize {
        if let Some(index) = self
            .pointers
            .iter()
            .position(|p| p.device_id == device_id && p.touch == touch)
        {
            index
        } else {
            let index = self.pointers.len();
            self.pointers.push(Pointer::new(device_id, touch));
            index
        }
    }

    pub fn enter(&mut self, device_id: DeviceId) {
        let device = self.index(device_id, false);
        let pointer = &self.pointers[device];
        self.engine.send_pointer_event(
            device as i32 + 10,
            FlutterPointerPhase::Add,
            pointer.position,
            FlutterPointerSignalKind::None,
            (0.0, 0.0),
            FlutterPointerDeviceKind::Mouse,
            FlutterPointerMouseButtons::Primary,
        );
    }

    pub fn leave(&mut self, device_id: DeviceId) {
        let device = self.index(device_id, false);
        let pointer = &self.pointers[device];
        self.engine.send_pointer_event(
            device as i32 + 10,
            FlutterPointerPhase::Remove,
            pointer.position,
            FlutterPointerSignalKind::None,
            (0.0, 0.0),
            FlutterPointerDeviceKind::Mouse,
            FlutterPointerMouseButtons::Primary,
        );
    }

    pub fn moved(&mut self, device_id: DeviceId, position: (f64, f64)) {
        let device = self.index(device_id, false);
        self.pointers[device].position = position;
        let pointer = &self.pointers[device];
        let phase = if pointer.pressed == 0 {
            FlutterPointerPhase::Hover
        } else {
            FlutterPointerPhase::Move
        };
        self.engine.send_pointer_event(
            device as i32 + 10,
            phase,
            pointer.position,
            FlutterPointerSignalKind::None,
            (0.0, 0.0),
            FlutterPointerDeviceKind::Mouse,
            FlutterPointerMouseButtons::Primary,
        );
    }

    pub fn input(&mut self, device_id: DeviceId, state: ElementState, button: MouseButton) {
        let device = self.index(device_id, false);
        match state {
            ElementState::Pressed => self.pointers[device].pressed += 1,
            ElementState::Released => self.pointers[device].pressed -= 1,
        }
        let pointer = &self.pointers[device];
        let phase = match state {
            ElementState::Pressed => FlutterPointerPhase::Down,
            ElementState::Released => FlutterPointerPhase::Up,
        };
        let button = match button {
            MouseButton::Left => FlutterPointerMouseButtons::Primary,
            MouseButton::Right => FlutterPointerMouseButtons::Secondary,
            MouseButton::Middle => FlutterPointerMouseButtons::Middle,
            MouseButton::Other(4) => FlutterPointerMouseButtons::Back,
            MouseButton::Other(5) => FlutterPointerMouseButtons::Forward,
            _ => FlutterPointerMouseButtons::Primary,
        };
        self.engine.send_pointer_event(
            device as i32 + 10,
            phase,
            pointer.position,
            FlutterPointerSignalKind::None,
            (0.0, 0.0),
            FlutterPointerDeviceKind::Mouse,
            button,
        );
    }

    pub fn wheel(&mut self, device_id: DeviceId, delta: (f64, f64)) {
        let device = self.index(device_id, false);
        let pointer = &self.pointers[device];
        let phase = if pointer.pressed == 0 {
            FlutterPointerPhase::Hover
        } else {
            FlutterPointerPhase::Move
        };
        self.engine.send_pointer_event(
            device as i32 + 10,
            phase,
            pointer.position,
            FlutterPointerSignalKind::Scroll,
            delta,
            FlutterPointerDeviceKind::Mouse,
            FlutterPointerMouseButtons::Primary,
        );
    }

    pub fn touch(&mut self, device_id: DeviceId, phase: TouchPhase, position: (f64, f64)) {
        let device = self.index(device_id, true);
        let phase = match phase {
            TouchPhase::Started => {
                self.pointers[device].pressed += 1;
                FlutterPointerPhase::Down
            }
            TouchPhase::Moved => FlutterPointerPhase::Move,
            TouchPhase::Ended => FlutterPointerPhase::Up,
            TouchPhase::Cancelled => FlutterPointerPhase::Cancel,
        };
        self.engine.send_pointer_event(
            self.pointers[device].pressed as i32 - 1,
            phase,
            position,
            FlutterPointerSignalKind::None,
            (0.0, 0.0),
            FlutterPointerDeviceKind::Touch,
            FlutterPointerMouseButtons::Primary,
        );
        if phase == FlutterPointerPhase::Up || phase == FlutterPointerPhase::Cancel {
            self.pointers[device].pressed -= 1;
        }
    }
}
