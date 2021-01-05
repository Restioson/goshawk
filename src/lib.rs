use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use std::ops::RangeInclusive;
use std::borrow::Cow;
use std::f32::consts::TAU;

// TODO validate settings (e.g ranges)

const SCROLL_TICK_GRACE_SECS: f64 = 0.05;

/// The system which manages the RTS camera state and manipulates the attached camera transform.
pub fn rts_camera_system(
    time: Res<Time>,
    windows: Res<Windows>,
    cursor_scroll_events: Res<Events<MouseWheel>>,
    keyboard: Res<Input<KeyCode>>,
    mut query: Query<(&mut RtsCamera, &mut Transform, Option<&ZoomSettings>, Option<&PanSettings>, Option<&TurnSettings>)>,
) {
    static DEFAULT_ZOOM: ZoomSettings = ZoomSettings::new();
    static DEFAULT_PAN: PanSettings = PanSettings::new();
    static DEFAULT_TURN: TurnSettings = TurnSettings::new();

    for (mut camera, mut transform, zoom, pan, turn) in query.iter_mut() {
        let window = windows.get_primary().unwrap();
        let cursor = match window.cursor_position() {
            Some(pos) => pos,
            None => return,
        };

        let zoom = zoom.unwrap_or(&DEFAULT_ZOOM);
        let pan = pan.unwrap_or(&DEFAULT_PAN);
        let turn = turn.unwrap_or(&DEFAULT_TURN);

        // TODO handle pixel units
        let scroll = camera.cursor_scroll_event_reader.latest(&cursor_scroll_events).map(|e| e.y);

        camera.tick(scroll, cursor, window, &keyboard, zoom, pan, turn, &time);
        *transform = camera.camera_transform();
    }
}


pub struct RtsCamera {
    /// Where the camera is looking (its target)
    pub looking_at: Vec3,
    /// The rotation of the camera. This is updated from the zoom distance and zoom settings, as well
    /// as the turn angle and turn settings. This must **not** be modified directly by the user.
    /// Rather, modify the yaw and pitch settings directly.
    pub rotation: Quat,
    /// The angle which the camera has turned to the right in radians
    pub yaw: f32,
    /// The velocity at which the camera is zooming in or out
    pub zoom_velocity: f32,
    /// The velocity at which the camera is panning
    pub pan_velocity: Vec2,
    pub turn_velocity: f32,
    /// The last time the scroll wheel sent a scroll event. It is treated as still having sent input
    /// for 0.05s after the last event, as otherwise idle deceleration kicks in too soon and scrolling
    /// is too slow.
    pub last_scroll_sec: f64,
    /// The distance which the camera is from the target
    pub zoom_distance: f32,
    pub cursor_scroll_event_reader: EventReader<MouseWheel>,
}

impl Default for RtsCamera {
    fn default() -> Self {
        RtsCamera {
            looking_at: Vec3::zero(),
            rotation: Quat::default(),
            yaw: 0.0,
            zoom_velocity: 0.0,
            pan_velocity: Vec2::zero(),
            turn_velocity: 0.0,
            last_scroll_sec: 0.0,
            zoom_distance: 10.0,
            cursor_scroll_event_reader: EventReader::default(),
        }
    }
}

impl RtsCamera {
    fn camera_translation(&self) -> Vec3 {
        self.looking_at + self.rotation * Vec3::new(0.0, 0.0, self.zoom_distance)
    }

    fn camera_transform(&self) -> Transform {
        let mat4 = Mat4::from_rotation_translation(self.rotation, self.camera_translation());
        Transform::from_matrix(mat4)
    }

    fn rotate(&mut self, angle: f32) {
        self.yaw += angle;

        if self.yaw > TAU {
            self.yaw -= TAU;
        }

        if self.yaw < 0.0 {
            self.yaw += TAU;
        }

        let rotation_y = Quat::from_rotation_y(angle);
        let camera_translation = self.camera_translation();
        self.looking_at = (rotation_y * (self.looking_at - camera_translation)) + camera_translation;
    }

    fn tick(
        &mut self,
        scroll: Option<f32>,
        cursor: Vec2,
        window: &Window,
        keyboard: &Input<KeyCode>,
        zoom: &ZoomSettings,
        pan: &PanSettings,
        turn: &TurnSettings,
        time: &Time,
    ) {
        let (delta, now) = (time.delta_seconds(), time.seconds_since_startup());
        let [mut x_decel, mut y_decel, mut turn_decel]: [Deceleration; 3] = Default::default();

        let mut zoom_decel = if (now - self.last_scroll_sec) < SCROLL_TICK_GRACE_SECS {
            Deceleration { pos: false, neg: false }
        } else {
            Deceleration { pos: true, neg: true }
        };

        if cursor.x < pan.mouse_accel_margin {
            if cursor.y > window.height() * (1.0 - turn.mouse_turn_margin) {
                self.turn_velocity += turn.mouse_accel * delta;
                turn_decel.pos = false;
            } else {
                self.pan_velocity.x -= pan.mouse_accel * delta;
                x_decel.neg = false;
            }
        } else if cursor.x > window.width() as f32 - pan.mouse_accel_margin {
            if cursor.y > window.height() * (1.0 - turn.mouse_turn_margin) {
                self.turn_velocity -= turn.mouse_accel * delta;
                turn_decel.neg = false;
            } else {
                self.pan_velocity.x += pan.mouse_accel * delta;
                x_decel.pos = false;
            }
        }

        if cursor.y < pan.mouse_accel_margin {
            self.pan_velocity.y -= pan.mouse_accel * delta;
            y_decel.neg = false;
        } else if cursor.y > window.height() as f32 - pan.mouse_accel_margin {
            self.pan_velocity.y += pan.mouse_accel * delta;
            y_decel.pos = false;
        }

        if pan.right_keys.iter().any(|c| keyboard.pressed(*c)) {
            self.pan_velocity.x += pan.keyboard_accel * delta;
            x_decel.pos = false;
        }

        if pan.left_keys.iter().any(|c| keyboard.pressed(*c)) {
            self.pan_velocity.x += -pan.keyboard_accel * delta;
            x_decel.neg = false;
        }

        if pan.up_keys.iter().any(|c| keyboard.pressed(*c)) {
            self.pan_velocity.y += pan.keyboard_accel * delta;
            y_decel.pos = false;
        }

        if pan.down_keys.iter().any(|c| keyboard.pressed(*c)) {
            self.pan_velocity.y += -pan.keyboard_accel * delta;
            y_decel.neg = false;
        }

        if turn.right_keys.iter().any(|c| keyboard.pressed(*c)) {
            self.turn_velocity -= turn.keyboard_accel * delta;
            turn_decel.neg = false;
        }

        if turn.left_keys.iter().any(|c| keyboard.pressed(*c)) {
            self.turn_velocity += turn.keyboard_accel * delta;
            turn_decel.pos = false;
        }

        if let Some(y) = scroll {
            if y > 0.0 {
                zoom_decel.pos = false;
            } else {
                zoom_decel.neg = false;
            }

            self.zoom_velocity -= y * zoom.scroll_accel;
            self.last_scroll_sec = now;
        }

        if zoom.zoom_in_keys.iter().any(|c| keyboard.pressed(*c)) {
            self.zoom_velocity -= zoom.keyboard_accel * delta;
            zoom_decel.pos = false;
        }

        if zoom.zoom_out_keys.iter().any(|c| keyboard.pressed(*c)) {
            self.zoom_velocity += zoom.keyboard_accel * delta;
            zoom_decel.neg = false;
        }

        // Apply zoom/pan deceleration
        turn_decel.apply(&mut self.turn_velocity, turn.idle_deceleration, delta);
        zoom_decel.apply(&mut self.zoom_velocity, zoom.idle_deceleration, delta);
        x_decel.apply(&mut self.pan_velocity.x, pan.idle_deceleration, delta);
        y_decel.apply(&mut self.pan_velocity.y, pan.idle_deceleration, delta);

        // Clamp velocity to max
        if self.pan_velocity.length_squared() > (pan.max_speed * pan.max_speed) {
            self.pan_velocity = pan.max_speed * self.pan_velocity.normalize();
        }

        self.zoom_velocity = f32::min(self.zoom_velocity, zoom.max_velocity);
        self.turn_velocity = clamp(self.turn_velocity, &(-turn.max_speed..=turn.max_speed));

        // Apply zoom velocity
        self.zoom_distance += self.zoom_velocity * delta;
        self.zoom_distance = clamp(self.zoom_distance, &zoom.distance_range);

        // Apply turn velocity
        self.rotate(self.turn_velocity * delta);
        self.yaw = clamp(self.yaw, &turn.yaw_range);

        // Rotate camera angle depending on zoom (pitch) and yaw
        let pitch = lerp_in_zone(self.zoom_distance, &zoom.angle_change_zone, &zoom.angle_range);
        self.rotation = Quat::from_rotation_ypr(self.yaw, -pitch, 0.0);

        // Apply pan velocity, taking into account the rotation of the camera
        let forward = Quat::from_rotation_y(self.yaw);
        let distance_factor = lerp_in_zone(self.zoom_distance, &zoom.angle_range, &pan.pan_speed_zoom_factor_range);
        self.looking_at += forward * (Vec3::unit_x() * self.pan_velocity.x * delta) * distance_factor;
        self.looking_at += forward * (-Vec3::unit_z() * self.pan_velocity.y * delta) * distance_factor;
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct ZoomSettings {
    /// The minimum and maximum angle in radians from the target
    pub angle_range: RangeInclusive<f32>,

    /// At the minimum distance, the angle to the target will be equal to `min_angle`, and vice versa.
    /// In other words, at these points the camera will only zoom in or out rather than also changing
    /// its angle - the angle only changes within this distance zone.
    pub angle_change_zone: RangeInclusive<f32>,

    /// The minimum and maximum distance from the target
    pub distance_range: RangeInclusive<f32>,

    /// The current velocity at which the camera is zooming in or out
    pub velocity: f32,
    /// The maximum velocity at which the camera can zoom in or out
    pub max_velocity: f32,
    /// The acceleration which the scroll wheel applies to the camera zoom while scrolling. Note
    /// that because of the discrete way in which scroll events are sent to the application,
    /// the delta time is *not* multiplied to the scroll accel value before it is added to the
    /// velocity. Therefore, this acts as the change in velocity per line or pixel scrolled, rather
    /// than the acceleration applied over a second of input.
    pub scroll_accel: f32,
    /// The acceleration which the keyboard applies to the camera zoom while scrolling
    pub keyboard_accel: f32,
    /// The deceleration of the camera zoom while nothing is causing it to zoom in or out
    pub idle_deceleration: f32,

    /// Keys which will cause the camera to zoom in
    pub zoom_in_keys: Cow<'static, [KeyCode]>,
    /// Keys which will cause the camera to zoom out
    pub zoom_out_keys: Cow<'static, [KeyCode]>,
}

impl ZoomSettings {
    pub const fn new() -> Self {
        ZoomSettings {
            angle_range: 0.5705693..=1.1637539,
            angle_change_zone: 5.0..=100.0,
            distance_range: 5.0..=100.0,
            velocity: 0.0,
            max_velocity: 5.0,
            scroll_accel: 5.0,
            keyboard_accel: 5.0,
            idle_deceleration: 5.0,
            zoom_in_keys: Cow::Borrowed(&[KeyCode::Equals, KeyCode::NumpadAdd]),
            zoom_out_keys: Cow::Borrowed(&[KeyCode::NumpadSubtract, KeyCode::Minus]),
        }
    }
}

impl Default for ZoomSettings {
    fn default() -> Self { ZoomSettings::new() }
}

#[derive(Clone, PartialEq, Debug)]
pub struct PanSettings {
    /// The acceleration which the mouse applies to the camera's panning motion.
    pub mouse_accel: f32,
    /// The minimum distance from the edge of the window the mouse must be in order for the camera
    /// to begin panning.
    pub mouse_accel_margin: f32,
    /// The acceleration that they keyboard applies to the camera's panning motion
    pub keyboard_accel: f32,
    /// The maximum velocity at which the camera may pan
    pub max_speed: f32,
    /// The deceleration of the panning while nothing is accelerating it in a certain direction
    pub idle_deceleration: f32,

    /// The effect of zoom distance on pan speed. This can be set to make panning faster when more
    /// zoomed out. The start value of this range is the factor at the minimum zoom level, and the
    /// end is the factor at the maximum zoom level. The factor will be linearly interpolated
    /// according to the zoom distance.
    pub pan_speed_zoom_factor_range: RangeInclusive<f32>,

    /// The keys which will cause the camera to pan left
    pub left_keys: Cow<'static, [KeyCode]>,
    /// The keys which will cause the camera to pan right
    pub right_keys: Cow<'static, [KeyCode]>,
    /// The keys which will cause the camera to pan up
    pub up_keys: Cow<'static, [KeyCode]>,
    /// The keys which will cause the camera to pan down
    pub down_keys: Cow<'static, [KeyCode]>,
}

impl PanSettings {
    pub const fn new() -> Self {
        PanSettings {
            mouse_accel: 15.0,
            mouse_accel_margin: 10.0,
            keyboard_accel: 5.0,
            max_speed: 5.0,
            idle_deceleration: 17.5,
            pan_speed_zoom_factor_range: 1.0..=2.0,
            left_keys: Cow::Borrowed(&[KeyCode::Left, KeyCode::A]),
            right_keys: Cow::Borrowed(&[KeyCode::Right, KeyCode::D]),
            up_keys: Cow::Borrowed(&[KeyCode::Up, KeyCode::W]),
            down_keys: Cow::Borrowed(&[KeyCode::Down, KeyCode::S]),
        }
    }
}

impl Default for PanSettings {
    fn default() -> Self { PanSettings::new() }
}

pub struct TurnSettings {
    /// The distance that the mouse must be from the top of the screen before it will start turning,
    /// provided that it is within the pan settings margin. This is measured as a ratio of the height
    /// dimension of the screen.
    pub mouse_turn_margin: f32,
    /// The range of yaw that the camera may turn, in radians
    pub yaw_range: RangeInclusive<f32>,
    /// The acceleration which the mouse applies to the camera's turning velocity (measured in
    /// radians per seconds squared)
    pub mouse_accel: f32,
    /// The acceleration which the keyboard applies to the camera's turning velocity (measured in
    /// radians per seconds squared)
    pub keyboard_accel: f32,
    pub max_speed: f32,
    pub idle_deceleration: f32,
    /// The keys which will cause the camera to turn left
    pub left_keys: Cow<'static, [KeyCode]>,
    /// The keys which will cause the camera to turn right
    pub right_keys: Cow<'static, [KeyCode]>,
}

impl TurnSettings {
    pub const fn new() -> Self {
        TurnSettings {
            mouse_turn_margin: 0.25,
            yaw_range: 0.0..=TAU,
            mouse_accel: 0.3,
            keyboard_accel: 1.8,
            max_speed: 1.5,
            idle_deceleration: 5.0,
            left_keys: Cow::Borrowed(&[KeyCode::Q]),
            right_keys: Cow::Borrowed(&[KeyCode::E]),
        }
    }
}

impl Default for TurnSettings {
    fn default() -> Self { TurnSettings::new() }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct Deceleration {
    /// Decelerate against motion in the positive direction
    pos: bool,
    /// Decelerate against motion in the negative direction
    neg: bool,
}

impl Default for Deceleration {
    fn default() -> Self {
        Deceleration { pos: true, neg: true }
    }
}

impl Deceleration {
    fn apply(&self, velocity: &mut f32, magnitude: f32, delta: f32) {
        if *velocity == 0.0 {
            return;
        }

        let signum = if self.pos && self.neg {
            -velocity.signum()
        } else if self.pos {
            -1.0
        } else if self.neg {
            1.0
        } else {
            return; // no deceleration required
        };

        let max_decel = magnitude * delta;
        let decel_magnitude = f32::min(max_decel.abs(), velocity.abs());

        *velocity += decel_magnitude * signum;
    }
}

#[must_use = "clamp returns the new value and does not modify the original"]
fn clamp(x: f32, range: &RangeInclusive<f32>) -> f32 {
    if x > *range.end() {
        *range.end()
    } else if x < *range.start() {
        *range.start()
    } else {
        x
    }
}

#[must_use = "lerp_in_zone returns the new value and does not modify the original"]
fn lerp_in_zone(val: f32, zone: &RangeInclusive<f32>, values: &RangeInclusive<f32>) -> f32 {
    let in_zone = clamp(val, zone);
    let normalised = (in_zone - *zone.start()) / (*zone.end() - *zone.start());
    normalised * (values.end() - values.start()) + values.start()
}
