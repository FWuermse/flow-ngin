#![cfg(all(feature = "integration-tests", not(target_arch = "wasm32")))]

use flow_ngin::{
    context::Context,
    flow::{GraphicsFlow, ImageTestResult, Out, run},
    pick::PickId,
    resources::{Asset, AssetSource, LoadErr},
};

#[derive(Default)]
struct State {
    completed: [usize; 2],
    frames: usize,
    seen: [[bool; 5]; 2],
    assets: Vec<Asset>,
}

// Intentionally neither Clone nor Default: completions move request identity.
struct Event {
    recipient: usize,
    request: usize,
}

struct Loader(usize);

impl Loader {
    fn batch(&self, request: usize, paths: &[(&str, u32)]) -> Out<State, Event> {
        Out::LoadBatch {
            paths: paths
                .iter()
                .map(|(path, id)| ((*path).into(), PickId(*id)))
                .collect(),
            event: Event {
                recipient: self.0,
                request,
            },
        }
    }
}

impl GraphicsFlow<State, Event> for Loader {
    fn on_init(&mut self, _: &mut Context, _: &mut State) -> Out<State, Event> {
        Out::Composed(vec![
            Out::Load {
                path: "metal.bin".into(),
                pick_id: PickId(1),
            },
            Out::Composed(vec![
                self.batch(
                    0,
                    &[
                        ("cube.obj", 1),
                        ("metal.gltf", 40 + self.0 as u32),
                        ("metal.bin", 2),
                    ],
                ),
                self.batch(1, &[("metal.gltf", 80), ("metal.gltf", 81)]),
                self.batch(2, &[]),
            ]),
        ])
    }

    fn on_load(
        &mut self,
        _: &Context,
        state: &mut State,
        event: Option<Event>,
        result: Result<Vec<Asset>, Vec<LoadErr>>,
    ) -> Out<State, Event> {
        state.completed[self.0] += 1;
        assert!(state.completed[self.0] <= 8);
        let Some(event) = event else {
            let mut assets = match result {
                Ok(assets) => assets,
                Err(errors) => {
                    assert_eq!(errors.len(), 1);
                    assert_eq!(errors[0].pick_id, PickId(1));
                    assert_eq!(errors[0].path, format!("missing-single-{}.bin", self.0));
                    return Out::Empty;
                }
            };
            assert_eq!(assets.len(), 1);
            let asset = assets.pop().unwrap();
            assert_eq!(asset.path, "metal.bin");
            assert_eq!(asset.pick_id, PickId(1));
            assert!(matches!(&asset.source, AssetSource::Bytes(bytes) if !bytes.is_empty()));
            state.assets.push(asset);
            return Out::Empty;
        };
        assert_eq!(event.recipient, self.0);
        assert!(!state.seen[self.0][event.request], "duplicate completion");
        state.seen[self.0][event.request] = true;
        match event.request {
            0 | 1 => {
                let assets = result.unwrap();
                let expected = if event.request == 0 {
                    vec!["cube.obj", "metal.gltf", "metal.bin"]
                } else {
                    vec!["metal.gltf", "metal.gltf"]
                };
                assert_eq!(
                    assets
                        .iter()
                        .map(|asset| asset.path.as_str())
                        .collect::<Vec<_>>(),
                    expected
                );
                for (index, asset) in assets.into_iter().enumerate() {
                    let expected_id = if event.request == 0 {
                        [1, 40 + self.0 as u32, 2][index]
                    } else {
                        80 + index as u32
                    };
                    assert_eq!(asset.pick_id, PickId(expected_id));
                    match &asset.source {
                        AssetSource::Scene(scene) => {
                            let id = if event.request == 0 {
                                40 + self.0 as u32
                            } else {
                                80 + index as u32
                            };
                            let renders = scene.get_renders();
                            assert!(!renders.is_empty());
                            assert!(renders.iter().all(|render| render.id == PickId(id)));
                        }
                        AssetSource::Model(_) => assert_eq!(index, 0),
                        AssetSource::Bytes(bytes) => {
                            assert_eq!(index, 2);
                            assert!(!bytes.is_empty());
                        }
                    }
                    state.assets.push(asset);
                }
                if event.request == 0 {
                    return Out::Composed(vec![Out::Composed(vec![self.batch(
                        3,
                        &[
                            ("missing-flow-first.bin", 1),
                            ("metal.bin", 2),
                            ("missing-flow-second.glb", 3),
                        ],
                    )])]);
                }
            }
            2 | 4 => assert!(result.unwrap().is_empty()),
            3 => {
                let Err(errors) = result else {
                    panic!("expected errors")
                };
                assert_eq!(
                    errors
                        .iter()
                        .map(|error| error.path.as_str())
                        .collect::<Vec<_>>(),
                    ["missing-flow-first.bin", "missing-flow-second.glb"]
                );
                assert_eq!(
                    errors.iter().map(|error| error.pick_id).collect::<Vec<_>>(),
                    [PickId(1), PickId(3)]
                );
                return Out::Composed(vec![
                    self.batch(4, &[]),
                    Out::Load {
                        path: format!("missing-single-{}.bin", self.0),
                        pick_id: PickId(1),
                    },
                    Out::Load {
                        path: "metal.bin".into(),
                        pick_id: PickId(1),
                    },
                ]);
            }
            _ => panic!("unexpected request"),
        }
        Out::Empty
    }

    fn on_custom_events(&mut self, _: &Context, _: &mut State, _: Event) -> Option<Event> {
        panic!("load completion entered custom-event chain")
    }

    fn on_update(
        &mut self,
        _: &Context,
        state: &mut State,
        _: instant::Duration,
    ) -> Out<State, Event> {
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
        Ok(
            if state.completed == [8, 8]
                && state.seen == [[true; 5]; 2]
                && state.assets.len() == 14
                && state.frames >= 20
            {
                ImageTestResult::Passed
            } else {
                ImageTestResult::Waiting
            },
        )
    }
}

#[test]
fn completion_stays_with_requester() {
    run::<State, Event>(vec![
        Box::new(|_| {
            Box::pin(async { Box::new(Loader(0)) as Box<dyn GraphicsFlow<State, Event>> })
        }),
        Box::new(|_| {
            Box::pin(async { Box::new(Loader(1)) as Box<dyn GraphicsFlow<State, Event>> })
        }),
    ])
    .unwrap();
}
