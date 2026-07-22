use gc::{Gc, GcCell};

use simiscript::interpreter::Interpreter;
use simiscript::lexer::{LexError, Token, TokenKind, lex};
use simiscript::native::{
    install_prelude, list_append, list_extend, list_get, list_length, list_set,
};
use simiscript::parser::{ParseError, Parser, parse};
use simiscript::runtime::{
    Environment, FloatKey, List, NativeFn, NativeFunction, NativeResult, Raised, RuntimeError,
    RuntimeResult, ScriptResult, SharedFunction, SharedList, SharedTable, TableKey, TraceFrame,
    UserFunction, Value,
};
use simiscript::{
    Raised as RootRaised, ScriptResult as RootScriptResult, TraceFrame as RootTraceFrame,
    Value as RootValue,
};

#[test]
fn existing_public_paths_remain_available() {
    let _ = lex as fn(&str) -> Result<Vec<Token>, LexError>;
    let _ = parse as fn(Vec<Token>) -> Result<simiscript::ast::Program, ParseError>;
    let _ = Parser::new;
    let _ = Interpreter::new;
    let _ = install_prelude;
    let _ = [list_length, list_get, list_append, list_extend, list_set];
    let _: Option<TokenKind> = None;
    let _: Option<Environment> = None;
    let _: Option<FloatKey> = None;
    let _: List = List::new(Vec::new());
    let _: SharedList = List::shared(Vec::new());
    let _: SharedTable = Gc::new(GcCell::new(Vec::new()));
    let _: Option<NativeFn> = None;
    let _: Option<NativeFunction> = None;
    let _: Option<NativeResult> = None;
    let _: Option<Raised> = None;
    let _: Option<RuntimeError> = None;
    let _: Option<RuntimeResult<Value>> = None;
    let _: Option<ScriptResult> = None;
    let _: Option<SharedFunction> = None;
    let _: Option<SharedList> = None;
    let _: Option<SharedTable> = None;
    let _: Option<TableKey> = None;
    let _: Option<TraceFrame> = None;
    let _: Option<UserFunction> = None;
    let _: Option<RootRaised> = None;
    let _: Option<RootScriptResult> = None;
    let _: Option<RootTraceFrame> = None;
    let _: Option<RootValue> = None;
}
