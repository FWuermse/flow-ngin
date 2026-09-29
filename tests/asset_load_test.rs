#![cfg(all(feature = "integration-tests", not(target_arch = "wasm32")))]

use flow_ngin::{
    context::Context,
    flow::{GraphicsFlow, ImageTestResult, Out, run},
    pick::PickId,
    resources::{Asset, LoadErr},
};

#[derive(Default)]
struct State {
    completed: [usize; 2],
    frames: usize,
}

struct Loader(usize);

impl GraphicsFlow<State, ()> for Loader {
    fn on_init(&mut self, _: &mut Context, _: &mut State) -> Out<State, ()> {
        Out::Composed(vec![
            Out::Load {
                path: "metal.gltf".into(),
                pick_id: PickId(40 + self.0 as u32),
            },
            Out::Composed(vec![Out::Load {
                path: "metal.bin".into(),
                pick_id: PickId(1),
            }]),
        ])
    }

    fn on_load(
        &mut self,
        _: &Context,
        state: &mut State,
        result: Result<(String, Asset), LoadErr>,
    ) -> Out<State, ()> {
        state.completed[self.0] += 1;
        assert!(state.completed[self.0] <= 3);
        match result {
            Ok((path, Asset::Scene(scene))) => {
                assert_eq!(path, "metal.gltf");
                let renders = scene.get_renders();
                assert!(!renders.is_empty());
                assert!(
                    renders
                        .iter()
                        .all(|render| render.id == PickId(40 + self.0 as u32))
                );
                Out::Composed(vec![Out::Load {
                    path: format!("missing-flow-{}.bin", self.0),
                    pick_id: PickId(1),
                }])
            }
            Ok((path, Asset::Bytes(bytes))) => {
                assert_eq!(path, "metal.bin");
                assert!(!bytes.is_empty());
                Out::Empty
            }
            Err(error) => {
                assert_eq!(error.path, format!("missing-flow-{}.bin", self.0));
                Out::Empty
            }
            _ => panic!("unexpected asset"),
        }
    }

    fn on_custom_events(&mut self, _: &Context, _: &mut State, _: ()) -> Option<()> {
        panic!("load completion entered custom-event chain")
    }

    fn on_update(
        &mut self,
        _: &Context,
        state: &mut State,
        _: instant::Duration,
    ) -> Out<State, ()> {
        state.frames += 1;
        assert!(state.frames < 2000, "loads did not complete");
        Out::Empty
    }

    fn render_to_texture(
        &self,
        _: &Context,
        state: &mut State,
        _: &mut image::ImageBuffer<image::Rgba<u8>, wgpu::BufferView>,
    ) -> anyhow::Result<ImageTestResult> {
        Ok(if state.completed == [3, 3] && state.frames >= 20 {
            ImageTestResult::Passed
        } else {
            ImageTestResult::Waiting
        })
    }
}

#[test]
fn completion_stays_with_requester() {
    run::<State, ()>(vec![
        Box::new(|_| Box::pin(async { Box::new(Loader(0)) as Box<dyn GraphicsFlow<State, ()>> })),
        Box::new(|_| Box::pin(async { Box::new(Loader(1)) as Box<dyn GraphicsFlow<State, ()>> })),
    ])
    .unwrap();
}
