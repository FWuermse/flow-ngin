pub(crate) struct State {
    frame_counter: u32,
    init_invocations: u32,
    click_invocations: u32,
    update_invocations: u32,
    render_invocations: u32,
    pub dummy_state: String,
}
impl State {
    pub fn new() -> Self {
        Self {
            frame_counter: 0,
            init_invocations: 0,
            click_invocations: 0,
            update_invocations: 0,
            render_invocations: 0,
            dummy_state: String::new(),
        }
    }

    pub fn frame(&mut self) {
        self.frame_counter += 1;
    }

    pub fn init(&mut self) {
        self.init_invocations += 1;
    }

    pub fn click(&mut self) {
        self.click_invocations += 1;
    }

    pub fn update(&mut self) {
        self.update_invocations += 1;
    }

    pub fn frame_counter(&self) -> u32 {
        self.frame_counter
    }

    pub fn init_invocations(&self) -> u32 {
        self.init_invocations
    }

    pub fn update_invocations(&self) -> u32 {
        self.update_invocations
    }

    pub fn click_invocations(&self) -> u32 {
        self.click_invocations
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}