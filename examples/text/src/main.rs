use flow_ngin::ui::TextLabel;

struct State;
impl Default for State {
    fn default() -> Self {
        Self
    }
}

enum Event {}

fn main() {
    let _ = flow_ngin::flow::run(vec![
        TextLabel::new("Hello, flow-ngin! 🎮")
            .position(10.0, 10.0)
            .font_size(30.0)
            .line_height(42.0)
            .color([255, 255, 255])
            .into_constructor::<State, Event>(),
    ]);
}
