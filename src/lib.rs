pub mod camera;
pub mod data_structures;
pub mod pipelines;
pub mod resources;
pub mod flow;

fn run_flows() {
    flow::run().unwrap();
}
