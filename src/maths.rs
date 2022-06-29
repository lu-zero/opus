use std::mem::size_of;

// TODO: ask upstream for a trait for leading_zeros
pub trait ILog {
    fn ilog(&self) -> Self;
}

impl ILog for usize {
    fn ilog(&self) -> Self {
        size_of::<usize>() * 8 - self.leading_zeros() as usize
    }
}

impl ILog for i32 {
    fn ilog(&self) -> Self {
        (size_of::<Self>() * 8 - self.leading_zeros() as usize) as i32
    }
}
