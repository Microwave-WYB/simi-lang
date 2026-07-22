use gc::{Gc, GcCell, force_collect, stats};

use super::{List, MapKey, UserFunction, Value};
use crate::ast::Block;
use crate::environment::Environment;
use crate::span::Span;

fn collected_after(build_cycle: impl FnOnce()) {
    force_collect();
    let baseline = stats().bytes_allocated;

    build_cycle();
    assert!(
        stats().bytes_allocated > baseline,
        "cycle construction should allocate managed nodes"
    );

    force_collect();
    assert_eq!(
        stats().bytes_allocated,
        baseline,
        "unreachable cycle should be reclaimed"
    );
}

#[test]
fn language_allows_direct_and_indirect_cycles_when_result_is_acyclic() {
    let result = crate::eval(
        r#"
        let list = require("std/list")
        let direct = []
        list.append(direct, direct)

        let list_value = []
        let map_value = {}
        list.append(list_value, map_value)
        map_value.list = list_value
        nil
        "#,
    )
    .expect("cyclic source should have no hard diagnostic")
    .expect("cyclic source should not raise");
    assert!(matches!(result, Value::Nil));
    force_collect();
}

#[test]
fn host_held_value_roots_a_cycle_until_it_is_dropped() {
    force_collect();
    let baseline = stats().bytes_allocated;

    let list = List::shared(Vec::new());
    list.borrow_mut().push(Value::List(list.clone()));
    let root = Value::List(list.clone());
    drop(list);

    force_collect();
    {
        let Value::List(root_list) = &root else {
            unreachable!();
        };
        let values = root_list.borrow();
        let Value::List(self_reference) = values.get_cloned(0).expect("rooted cycle element")
        else {
            panic!("rooted cycle should remain accessible");
        };
        assert!(Gc::ptr_eq(root_list, &self_reference));
        assert!(stats().bytes_allocated > baseline);
    }

    drop(root);
    force_collect();
    assert_eq!(stats().bytes_allocated, baseline);
}

#[test]
fn unreachable_list_and_map_cycles_are_collected() {
    collected_after(|| {
        let list = List::shared(Vec::new());
        list.borrow_mut().push(Value::List(list.clone()));

        let map = Gc::new(GcCell::new(Vec::new()));
        map.borrow_mut()
            .push((MapKey::String("self".to_owned()), Value::Map(map.clone())));
    });
}

#[test]
fn unreachable_slice_view_and_cyclic_source_are_collected() {
    collected_after(|| {
        let source = List::shared(Vec::new());
        source.borrow_mut().push(Value::List(source.clone()));
        let view = source.borrow().slice(0, 1).into_shared();
        drop(source);
        drop(view);
    });
}

#[test]
fn unreachable_indirect_container_cycle_is_collected() {
    collected_after(|| {
        let list = List::shared(Vec::new());
        let map = Gc::new(GcCell::new(vec![(
            MapKey::String("list".to_owned()),
            Value::List(list.clone()),
        )]));
        list.borrow_mut().push(Value::Map(map));
    });
}

#[test]
fn unreachable_recursive_closure_environment_cycle_is_collected() {
    collected_after(|| {
        let environment = Environment::new();
        let function = Gc::new(UserFunction {
            name: "recursive".to_owned(),
            params: Vec::new(),
            body: Block {
                items: Vec::new(),
                span: Span::new(0, 0),
            },
            closure: environment.clone(),
        });
        environment.define("recursive", Value::Function(function));
    });
}
