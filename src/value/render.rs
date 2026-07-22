use std::collections::HashSet;

use gc::Gc;

use super::{MapKey, Value};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum ContainerKind {
    List,
    Map,
}

type ContainerId = (ContainerKind, usize);

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Int(_) => "integer",
            Self::Float(_) => "float",
            Self::String(_) => "string",
            Self::Bool(_) => "boolean",
            Self::Nil => "nil",
            Self::List(_) => "list",
            Self::Map(_) => "map",
            Self::Function(_) => "function",
            Self::NativeFunction(_) => "native function",
        }
    }

    pub fn render(&self) -> String {
        self.render_with(&mut HashSet::new())
    }

    fn render_with(&self, active: &mut HashSet<ContainerId>) -> String {
        match self {
            Self::Int(value) => value.to_string(),
            Self::Float(value) => render_float(*value),
            Self::String(value) => render_string(value),
            Self::Bool(value) => value.to_string(),
            Self::Nil => "nil".to_owned(),
            Self::List(values) => {
                let id = (ContainerKind::List, Gc::as_ptr(values) as usize);
                if !active.insert(id) {
                    return "<cycle>".to_owned();
                }

                let values = values.borrow();
                let rendered = values.with_visible(|values| {
                    values
                        .iter()
                        .map(|value| value.render_with(active))
                        .collect::<Vec<_>>()
                        .join(", ")
                });
                active.remove(&id);
                format!("[{rendered}]")
            }
            Self::Map(entries) => {
                let id = (ContainerKind::Map, Gc::as_ptr(entries) as usize);
                if !active.insert(id) {
                    return "<cycle>".to_owned();
                }

                let entries = entries.borrow();
                let rendered = entries
                    .iter()
                    .map(|(key, value)| {
                        format!("{}={}", render_map_key(key), value.render_with(active))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                active.remove(&id);
                format!("{{{rendered}}}")
            }
            Self::Function(function) => format!("<fn {}>", function.name),
            Self::NativeFunction(function) => format!("<native {}>", function.name),
        }
    }
}

fn render_map_key(key: &MapKey) -> String {
    match key {
        MapKey::String(value) if is_identifier(value) => value.clone(),
        MapKey::String(value) => format!("[{}]", render_string(value)),
        MapKey::Int(value) => format!("[{value}]"),
        MapKey::Float(value) => format!("[{}]", render_float(value.value())),
        MapKey::Bool(value) => format!("[{value}]"),
    }
}

fn render_float(value: f64) -> String {
    let rendered = value.to_string();
    if rendered.contains(['.', 'e', 'E']) {
        rendered
    } else {
        format!("{rendered}.0")
    }
}

fn is_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    matches!(characters.next(), Some('_' | 'a'..='z' | 'A'..='Z'))
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn render_string(value: &str) -> String {
    let mut rendered = String::with_capacity(value.len() + 2);
    rendered.push('"');

    for character in value.chars() {
        match character {
            '"' => rendered.push_str("\\\""),
            '\\' => rendered.push_str("\\\\"),
            '\n' => rendered.push_str("\\n"),
            '\r' => rendered.push_str("\\r"),
            '\t' => rendered.push_str("\\t"),
            character => rendered.push(character),
        }
    }

    rendered.push('"');
    rendered
}
