pub struct FixedBuffer<T, const N: usize> {
    buffer: [T; N],
}

impl<T, const N: usize> FixedBuffer<T, N> {
    pub fn capacity(&self) -> usize {
        N
    }
}
