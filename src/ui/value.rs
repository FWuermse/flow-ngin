use std::cell::RefCell;
use std::rc::Rc;

/// A typed, shared value cell for UI input widgets.
///
/// Widgets bind to a `Value<T>` and update it internally. The application
/// reads the cell on demand (e.g., when a submit button is clicked).
///
/// # Example
///
/// ```
/// use flow_ngin::ui::value::Value;
///
/// let username = Value::new(String::new());
/// assert_eq!(username.get(), "");
///
/// username.set("alice".into());
/// assert_eq!(username.get(), "alice");
///
/// // Clone shares the same cell
/// let handle = username.clone();
/// handle.set("bob".into());
/// assert_eq!(username.get(), "bob");
/// ```
#[derive(Debug)]
pub struct Value<T>(Rc<RefCell<T>>);

impl<T> Clone for Value<T> {
    fn clone(&self) -> Self {
        Value(Rc::clone(&self.0))
    }
}

impl<T> Value<T> {
    pub fn new(initial: T) -> Self {
        Value(Rc::new(RefCell::new(initial)))
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.0.borrow().clone()
    }

    pub fn set(&self, val: T) {
        *self.0.borrow_mut() = val;
    }
}
