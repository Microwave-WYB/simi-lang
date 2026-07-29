use std::sync::Arc;

/// An immutable, contiguous sequence of octets.
///
/// Cloning and valid slices retain the same backing allocation. Slices are relative to the
/// current visible range and use an exclusive end offset.
#[derive(Clone, Debug)]
pub struct Bytes {
    backing: Arc<[u8]>,
    start: usize,
    length: usize,
}

impl Bytes {
    pub fn new(values: Vec<u8>) -> Self {
        let length = values.len();
        Self {
            backing: Arc::from(values),
            start: 0,
            length,
        }
    }

    pub fn len(&self) -> usize {
        self.length
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.backing[self.start..self.start + self.length]
    }

    pub fn get(&self, index: usize) -> Option<u8> {
        self.as_slice().get(index).copied()
    }

    pub fn slice(&self, start: usize, end: usize) -> Option<Self> {
        (start <= end && end <= self.length).then(|| Self {
            backing: Arc::clone(&self.backing),
            start: self.start + start,
            length: end - start,
        })
    }
}

impl PartialEq for Bytes {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for Bytes {}

impl From<Vec<u8>> for Bytes {
    fn from(values: Vec<u8>) -> Self {
        Self::new(values)
    }
}

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn clones_and_slices_share_immutable_backing() {
        let source = Bytes::new(vec![0, 1, 2, 3]);
        let cloned = source.clone();
        let slice = source.slice(1, 3).expect("valid slice");
        let nested = slice.slice(1, 2).expect("valid nested slice");

        assert!(Arc::ptr_eq(&source.backing, &cloned.backing));
        assert!(Arc::ptr_eq(&source.backing, &slice.backing));
        assert!(Arc::ptr_eq(&source.backing, &nested.backing));
        assert_eq!(slice.as_slice(), [1, 2]);
        assert_eq!(nested.as_slice(), [2]);
        assert_eq!(nested, Bytes::new(vec![2]));
    }

    #[test]
    fn slices_validate_relative_exclusive_bounds() {
        let bytes = Bytes::new(vec![10, 20]);

        assert_eq!(
            bytes.slice(0, 0).map(|slice| slice.as_slice().to_vec()),
            Some(vec![])
        );
        assert_eq!(
            bytes.slice(0, 2).map(|slice| slice.as_slice().to_vec()),
            Some(vec![10, 20])
        );
        assert!(bytes.slice(2, 1).is_none());
        assert!(bytes.slice(0, 3).is_none());
    }
}
