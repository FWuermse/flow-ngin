use futures::{FutureExt, channel::oneshot};
use std::panic::AssertUnwindSafe;

use super::LoadingScreen;
use crate::{
    context::{Context, InitContext},
    flow::{FlowConstructor, FlowEvent, GraphicsFlow},
};

type Flows<S, E> = Vec<Box<dyn GraphicsFlow<S, E>>>;

pub(crate) struct Startup<S: 'static, E: 'static> {
    pub ready: oneshot::Receiver<std::thread::Result<Flows<S, E>>>,
    pub screen: Flows<S, E>,
    pub events: Vec<FlowEvent<S, E>>,
}

pub(super) async fn prepare_flows<S: 'static, E: Send + 'static>(
    constructors: Vec<FlowConstructor<S, E>>,
    gpu: InitContext,
) -> std::thread::Result<Flows<S, E>> {
    let prepare = futures::future::join_all(constructors.into_iter().map(|constructor| {
        let gpu = gpu.clone();
        async move {
            let mut flow = constructor(gpu.clone()).await;
            flow.on_init_async(gpu).await;
            flow
        }
    }));
    AssertUnwindSafe(prepare).catch_unwind().await
}

impl<S: 'static, E: Send + 'static> Startup<S, E> {
    pub fn new(
        ctx: &Context,
        constructors: Vec<FlowConstructor<S, E>>,
        proxy: winit::event_loop::EventLoopProxy<FlowEvent<S, E>>,
    ) -> (Self, impl Future<Output = ()> + 'static) {
        let (send, ready) = oneshot::channel();
        let gpu = ctx.into();
        let task = async move {
            let flows = prepare_flows(constructors, gpu).await;
            if send.send(flows).is_ok() {
                let _ = proxy.send_event(FlowEvent::StartupReady);
            }
        };
        (
            Self {
                ready,
                screen: vec![Box::new(LoadingScreen::new(ctx))],
                events: Vec::new(),
            },
            task,
        )
    }
}
