use crate::flow::GraphicsFlow;

/// Trait for UI elements that can be positioned by a container.
///
/// A container calls `resolve` with its own bounds; the implementor computes
/// its absolute pixel position from its declared alignment and updates its
/// GPU resources accordingly.
pub trait Layout {
    fn resolve(&mut self, parent_x: u32, parent_y: u32, parent_w: u32, parent_h: u32, queue: &wgpu::Queue);
}

/// Supertrait combining [`GraphicsFlow`] and [`Layout`].
///
/// Any type that implements both traits automatically implements `UIElement`
/// via the blanket impl below, making it eligible to be added to a [`Container`].
pub trait UIElement<S, E>: GraphicsFlow<S, E> + Layout {}

impl<T, S, E> UIElement<S, E> for T where T: GraphicsFlow<S, E> + Layout {}
