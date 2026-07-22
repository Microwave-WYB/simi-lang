use std::rc::Rc;

use gc::{Finalize, Gc, GcCell, Trace, custom_trace};

use super::{SharedList, Value};

pub struct List {
    backing: Gc<GcCell<Vec<Value>>>,
    views: Rc<()>,
    start: usize,
    length: usize,
}

impl Clone for List {
    fn clone(&self) -> Self {
        Self {
            backing: self.backing.clone(),
            views: Rc::clone(&self.views),
            start: self.start,
            length: self.length,
        }
    }
}

impl List {
    pub fn new(values: Vec<Value>) -> Self {
        let length = values.len();
        Self {
            backing: Gc::new(GcCell::new(values)),
            views: Rc::new(()),
            start: 0,
            length,
        }
    }

    pub fn shared(values: Vec<Value>) -> SharedList {
        Self::new(values).into_shared()
    }

    pub(crate) fn into_shared(self) -> SharedList {
        Gc::new(GcCell::new(self))
    }

    pub fn len(&self) -> usize {
        self.length
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub fn get_cloned(&self, index: usize) -> Option<Value> {
        self.with_visible(|values| values.get(index).cloned())
    }

    pub(crate) fn with_visible<R>(&self, read: impl FnOnce(&[Value]) -> R) -> R {
        let backing = self.backing.borrow();
        read(&backing[self.start..self.start + self.length])
    }

    pub fn to_vec(&self) -> Vec<Value> {
        self.with_visible(<[Value]>::to_vec)
    }

    pub fn push(&mut self, value: Value) {
        self.detach_if_needed();
        self.backing.borrow_mut().push(value);
        self.length += 1;
    }

    pub fn extend(&mut self, values: impl IntoIterator<Item = Value>) {
        let values = values.into_iter().collect::<Vec<_>>();
        self.detach_if_needed();
        self.length += values.len();
        self.backing.borrow_mut().extend(values);
    }

    pub(crate) fn insert(&mut self, index: usize, value: Value) {
        assert!(
            index <= self.length,
            "list insertion index is out of bounds"
        );
        self.detach_if_needed();
        self.backing.borrow_mut().insert(index, value);
        self.length += 1;
    }

    pub(crate) fn remove(&mut self, index: usize) -> Value {
        assert!(index < self.length, "list removal index is out of bounds");
        self.detach_if_needed();
        self.length -= 1;
        self.backing.borrow_mut().remove(index)
    }

    pub(crate) fn reverse(&mut self) {
        self.detach_if_needed();
        self.backing.borrow_mut().reverse();
    }

    pub fn set(&mut self, index: usize, value: Value) -> bool {
        if index >= self.length {
            return false;
        }
        self.detach_if_needed();
        self.backing.borrow_mut()[index] = value;
        true
    }

    pub(crate) fn suffix(&self, offset: usize) -> Self {
        self.slice(offset, self.length)
    }

    pub(crate) fn shallow_copy(&self) -> Self {
        self.slice(0, self.length)
    }

    pub(crate) fn slice(&self, start: usize, end: usize) -> Self {
        assert!(start <= end, "list slice ends before it starts");
        assert!(end <= self.length, "list slice ends past its source");
        Self {
            backing: self.backing.clone(),
            views: Rc::clone(&self.views),
            start: self.start + start,
            length: end - start,
        }
    }

    fn detach_if_needed(&mut self) {
        if self.start != 0
            || self.length != self.backing.borrow().len()
            || Rc::strong_count(&self.views) != 1
        {
            let values = self.to_vec();
            self.backing = Gc::new(GcCell::new(values));
            self.views = Rc::new(());
            self.start = 0;
        }
    }
}

impl Finalize for List {}
#[allow(unsafe_op_in_unsafe_fn)]
unsafe impl Trace for List {
    custom_trace!(this, {
        mark(&this.backing);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ints(values: &List) -> Vec<i64> {
        values.with_visible(|values| {
            values
                .iter()
                .map(|value| match value {
                    Value::Int(value) => *value,
                    _ => panic!("expected integer"),
                })
                .collect()
        })
    }

    #[test]
    fn unique_full_list_mutations_retain_backing_identity() {
        let mut values = List::new(vec![Value::Int(1), Value::Int(2)]);
        let original = values.backing.clone();

        assert!(values.set(0, Value::Int(7)));
        assert!(Gc::ptr_eq(&original, &values.backing));
        values.push(Value::Int(3));
        assert!(Gc::ptr_eq(&original, &values.backing));
        values.extend([Value::Int(4)]);
        assert!(Gc::ptr_eq(&original, &values.backing));
        assert_eq!(ints(&values), vec![7, 2, 3, 4]);
    }

    #[test]
    fn suffixes_share_backing_until_one_is_mutated() {
        let source = List::new(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let mut suffix = source.suffix(1);
        assert!(Gc::ptr_eq(&source.backing, &suffix.backing));
        assert_eq!(suffix.start, 1);
        assert_eq!(ints(&suffix), vec![2, 3]);

        suffix.set(0, Value::Int(9));
        assert!(!Gc::ptr_eq(&source.backing, &suffix.backing));
        assert_eq!(ints(&source), vec![1, 2, 3]);
        assert_eq!(ints(&suffix), vec![9, 3]);
        assert_eq!(suffix.start, 0);
    }

    #[test]
    fn chained_suffixes_do_not_copy_backing() {
        let source = List::new((0..2_000).map(Value::Int).collect());
        let original = source.backing.clone();
        let mut suffix = source;
        for _ in 0..1_000 {
            suffix = suffix.suffix(1);
            assert!(Gc::ptr_eq(&original, &suffix.backing));
        }
        assert_eq!(suffix.len(), 1_000);
        assert_eq!(
            suffix.get_cloned(0).map(|value| value.render()).as_deref(),
            Some("1000")
        );
    }

    #[test]
    fn shallow_copies_share_full_range_backing_until_mutation() {
        let mut source = List::new(vec![Value::Int(1), Value::Int(2)]);
        let mut independent = source.shallow_copy();
        assert!(Gc::ptr_eq(&source.backing, &independent.backing));
        assert_eq!(independent.start, 0);
        assert_eq!(independent.length, source.length);

        independent.set(0, Value::Int(7));
        assert!(!Gc::ptr_eq(&source.backing, &independent.backing));
        assert_eq!(ints(&source), vec![1, 2]);
        assert_eq!(ints(&independent), vec![7, 2]);

        source.push(Value::Int(3));
        assert_eq!(ints(&source), vec![1, 2, 3]);
        assert_eq!(ints(&independent), vec![7, 2]);
    }

    #[test]
    fn slices_share_backing_and_detach_for_structural_mutations() {
        let source = List::new(vec![
            Value::Int(0),
            Value::Int(1),
            Value::Int(2),
            Value::Int(3),
        ]);
        let original = source.backing.clone();
        let mut slice = source.slice(1, 3);
        assert!(Gc::ptr_eq(&original, &slice.backing));
        assert_eq!(ints(&slice), vec![1, 2]);

        slice.insert(1, Value::Int(9));
        assert!(!Gc::ptr_eq(&original, &slice.backing));
        assert_eq!(ints(&source), vec![0, 1, 2, 3]);
        assert_eq!(ints(&slice), vec![1, 9, 2]);
        assert_eq!(slice.remove(0).render(), "1");
        slice.reverse();
        assert_eq!(ints(&slice), vec![2, 9]);
    }
}
