#[derive(Clone, Debug)]
pub enum Keyframes {
    Translation(Vec<cgmath::Vector3<f32>>),
    Rotation(Vec<cgmath::Quaternion<f32>>),
    Scale(Vec<cgmath::Vector3<f32>>),
    Other,
}