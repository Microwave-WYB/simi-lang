use std::sync::Arc;

use gc::{Gc, GcCell};

use simiscript::interpreter::Interpreter;
use simiscript::lexer::{LexError, Token, TokenKind, lex};
use simiscript::native::{
    list_append, list_contains, list_extend, list_get, list_insert, list_length, list_pop,
    list_remove, list_reverse, list_set, list_slice,
};
use simiscript::parser::{ParseError, Parser, parse};
use simiscript::runtime::{
    Environment, FloatKey, List, MapKey, NativeFn, NativeFunction, NativeResult, Raised,
    RuntimeError, RuntimeResult, ScriptResult, SharedFunction, SharedList, SharedMap, TraceFrame,
    UserFunction, Value,
};
use simiscript::{
    Engine, EngineBuilder, Module, ModuleBuilder, NativeCallback, Raised as RootRaised,
    ScriptResult as RootScriptResult, TraceFrame as RootTraceFrame, Value as RootValue,
};

#[test]
fn existing_public_paths_remain_available() {
    let _ = lex as fn(&str) -> Result<Vec<Token>, LexError>;
    let _ = parse as fn(Vec<Token>) -> Result<simiscript::ast::Program, ParseError>;
    let _ = Parser::new;
    let _ = Interpreter::new;
    let _ = Engine::new;
    let _ = Engine::builder;
    let _ = EngineBuilder::new;
    let _: ModuleBuilder = Module::builder("example");
    let _: Option<&NativeCallback> = None;
    let _ = [
        list_length,
        list_get,
        list_append,
        list_extend,
        list_set,
        list_insert,
        list_remove,
        list_pop,
        list_slice,
        list_contains,
        list_reverse,
    ];
    let _: Option<TokenKind> = None;
    let _: Option<Environment> = None;
    let _: Option<FloatKey> = None;
    let _: List = List::new(Vec::new());
    let _: SharedList = List::shared(Vec::new());
    let _: SharedMap = Gc::new(GcCell::new(Vec::new()));
    let _: Option<NativeFn> = None;
    let native = NativeFunction::new("example.function", 1, Arc::new(list_length));
    assert_eq!(native.name(), "example.function");
    assert_eq!(native.arity(), 1);
    let _: Option<NativeResult> = None;
    let _: Option<Raised> = None;
    let _: Option<RuntimeError> = None;
    let _: Option<RuntimeResult<Value>> = None;
    let _: Option<ScriptResult> = None;
    let _: Option<SharedFunction> = None;
    let _: Option<SharedList> = None;
    let _: Option<SharedMap> = None;
    let _: Option<MapKey> = None;
    let _: Option<TraceFrame> = None;
    let _: Option<UserFunction> = None;
    let _: Option<RootRaised> = None;
    let _: Option<RootScriptResult> = None;
    let _: Option<RootTraceFrame> = None;
    let _: Option<RootValue> = None;
}
