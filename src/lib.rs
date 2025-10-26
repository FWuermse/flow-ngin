pub mod camera;
pub mod context;
pub mod data_structures;
pub mod pick;
pub mod pipelines;
pub mod resources;
pub mod flow;

fn run_flows() {
    flow::run().unwrap();
}
