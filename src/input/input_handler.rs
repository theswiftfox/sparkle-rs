#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Button {
    Left,
    Middle,
    Right,
}
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Down,
    Up,
}
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ScrollAxis {
    Vertical,
    Horizontal,
}
#[derive(Debug, PartialEq, Eq)]
pub enum ApplicationRequest {
    UnsnapMouse,
    SnapMouse,
    Nothing,
    Quit,
}

pub trait InputHandler {
    fn update(&mut self, delta_t: f32);
    fn handle_key(&mut self, key: Key, action: Action) -> ApplicationRequest;
    fn handle_mouse(&mut self, button: Button, action: Action) -> ApplicationRequest;
    fn handle_wheel(&mut self, axis: ScrollAxis, value: f32);
    fn handle_mouse_move(&mut self, x: i32, y: i32);
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Key {
    None,
    Q,
    W,
    E,
    R,
    T,
    Y,
    U,
    I,
    O,
    P,
    A,
    S,
    D,
    F,
    G,
    H,
    J,
    K,
    L,
    Z,
    X,
    C,
    V,
    B,
    N,
    M,
    Space,
    Backspace,
    Return,
    Apostrophe,
    Caps,
    Shift,
    CtrlL,
    CtrlR,
    ShiftR,
    KeyUp,
    KeyDown,
    KeyLeft,
    KeyRight,
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Zero,
    Minus,
    Equals,
    BracketL,
    BracketR,
    Semicolon,
    Dash,
    Slash,
    Backslash,
    Colon,
    Point,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Esc,
    PrntScr,
    Ins,
    Del,
}
