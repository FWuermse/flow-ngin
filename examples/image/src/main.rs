use flow_ngin::ui::image::Atlas;

struct State;
impl Default for State {
    fn default() -> Self {
        Self
    }
}

enum Event {}

fn main() {
    let atlas = Atlas::new("textures/log.png");
    let _ = flow_ngin::flow::run(vec![
        Icon::new(include_bytes!("../../../assets/textures/log.png").as_slice())
            .position(0.0, 0.0)
            .into_constructor::<State, Event>(),
    ]);
}
