pub enum Key {
    None,
    Q,W,E,R,T,Y,U,I,O,P,
    A,S,D,F,G,H,J,K,L,
    Z,X,C,V,B,N,M,
    Space,Backspace,Return,Apostrophe,Caps,Shift,CtrlL,CtrlR,ShiftR,
    KeyUp,KeyDown,KeyLeft,KeyRight,
    One,Two,Three,Four,Five,Six,Seven,Eight,Nine,Zero,
    Minus,Equals,BracketL,BracketR,Semicolon,Dash,Slash,
    Backslash,Colon,Point,
    F1,F2,F3,F4,F5,F6,F7,F8,F9,F10,F11,F12,Esc,PrntScr,Ins,Del,
}

pub enum Button {
    Left,
    Middle,
    Right,
}

pub enum Action {
    Down,
    Up,
}

pub enum Direction {
    Down,
    Up,
    Left,
    Right,
}

pub trait InputHandler {
    fn update(&self, delta_t: f32);
    fn handle_key(&self, key: Key, action: Action);
    fn handle_mouse(&self, button: Button, action: Action);
    fn handle_scroll(&self, direction: Direction, value: f32);
    fn handle_mouse_move(&self, x: f32, y: f32);
}
