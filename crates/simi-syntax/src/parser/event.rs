use rowan::{GreenNodeBuilder, Language};

use crate::lexer::Lexeme;
use crate::syntax::{SimiLanguage, SyntaxKind, SyntaxNode};

#[derive(Debug)]
pub(super) enum Event {
    Placeholder,
    Start {
        kind: SyntaxKind,
        forward_parent: Option<usize>,
    },
    Finish,
    Token(usize),
    TokenAs(usize, SyntaxKind),
}

#[derive(Debug)]
pub(super) struct Marker {
    position: usize,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CompletedMarker {
    position: usize,
}

impl Marker {
    pub(super) fn complete(self, events: &mut Vec<Event>, kind: SyntaxKind) -> CompletedMarker {
        events[self.position] = Event::Start {
            kind,
            forward_parent: None,
        };
        events.push(Event::Finish);
        CompletedMarker {
            position: self.position,
        }
    }
}

impl CompletedMarker {
    pub(super) const fn position(self) -> usize {
        self.position
    }

    pub(super) fn precede(self, events: &mut Vec<Event>) -> Marker {
        let position = events.len();
        events.push(Event::Placeholder);
        match &mut events[self.position] {
            Event::Start { forward_parent, .. } => *forward_parent = Some(position - self.position),
            _ => unreachable!("completed marker must point to a start event"),
        }
        Marker { position }
    }
}

pub(super) fn start(events: &mut Vec<Event>) -> Marker {
    let position = events.len();
    events.push(Event::Placeholder);
    Marker { position }
}

pub(super) fn build(mut events: Vec<Event>, lexemes: &[Lexeme]) -> SyntaxNode {
    let mut builder = GreenNodeBuilder::new();
    for index in 0..events.len() {
        match std::mem::replace(&mut events[index], Event::Placeholder) {
            Event::Placeholder => {}
            Event::Finish => builder.finish_node(),
            Event::Token(token) => {
                let lexeme = &lexemes[token];
                builder.token(SimiLanguage::kind_to_raw(lexeme.kind), &lexeme.text);
            }
            Event::TokenAs(token, kind) => {
                let lexeme = &lexemes[token];
                builder.token(SimiLanguage::kind_to_raw(kind), &lexeme.text);
            }
            Event::Start {
                kind,
                forward_parent,
            } => {
                let mut kinds = vec![kind];
                let mut next = forward_parent;
                let mut at = index;
                while let Some(distance) = next {
                    at += distance;
                    match std::mem::replace(&mut events[at], Event::Placeholder) {
                        Event::Start {
                            kind,
                            forward_parent,
                        } => {
                            kinds.push(kind);
                            next = forward_parent;
                        }
                        _ => unreachable!("forward parent must be a start event"),
                    }
                }
                for kind in kinds.into_iter().rev() {
                    builder.start_node(SimiLanguage::kind_to_raw(kind));
                }
            }
        }
    }
    SyntaxNode::new_root(builder.finish())
}
