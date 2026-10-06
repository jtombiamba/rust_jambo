mod paypal;

pub use paypal::{PaymentService, PaymentServiceTrait};

#[cfg(test)]
pub use paypal::{CaptureResult, OrderCreated};
