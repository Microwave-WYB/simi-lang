use std::io::{self, Write};

use crate::runtime::{NativeResult, Raised, Value};
use crate::span::Span;

pub(crate) fn stdin_readline(_: &[Value], span: Span) -> NativeResult {
    let mut line = String::new();
    match io::stdin().read_line(&mut line) {
        Ok(0) => Ok(Ok(Value::Nil)),
        Ok(_) => {
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            Ok(Ok(Value::String(line)))
        }
        Err(error) => Ok(Err(Raised::io_error("readline", error.to_string(), span))),
    }
}

pub(crate) fn stdout_print(args: &[Value], span: Span) -> NativeResult {
    write_stream(io::stdout(), &args[0], false, "print", span)
}

pub(crate) fn stdout_println(args: &[Value], span: Span) -> NativeResult {
    write_stream(io::stdout(), &args[0], true, "println", span)
}

pub(crate) fn stdout_flush(_: &[Value], span: Span) -> NativeResult {
    flush_stream(io::stdout(), "flush", span)
}

pub(crate) fn stderr_print(args: &[Value], span: Span) -> NativeResult {
    write_stream(io::stderr(), &args[0], false, "print", span)
}

pub(crate) fn stderr_println(args: &[Value], span: Span) -> NativeResult {
    write_stream(io::stderr(), &args[0], true, "println", span)
}

pub(crate) fn stderr_flush(_: &[Value], span: Span) -> NativeResult {
    flush_stream(io::stderr(), "flush", span)
}

fn write_stream(
    mut stream: impl Write,
    value: &Value,
    newline: bool,
    operation: &str,
    span: Span,
) -> NativeResult {
    let rendered = printable(value);
    let result = if newline {
        writeln!(stream, "{rendered}")
    } else {
        write!(stream, "{rendered}")
    }
    .and_then(|()| stream.flush());
    match result {
        Ok(()) => Ok(Ok(Value::Nil)),
        Err(error) => Ok(Err(Raised::io_error(operation, error.to_string(), span))),
    }
}

fn flush_stream(mut stream: impl Write, operation: &str, span: Span) -> NativeResult {
    match stream.flush() {
        Ok(()) => Ok(Ok(Value::Nil)),
        Err(error) => Ok(Err(Raised::io_error(operation, error.to_string(), span))),
    }
}

fn printable(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        value => value.render(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("closed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("closed"))
        }
    }

    struct FlushFailingWriter;

    impl Write for FlushFailingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("flush failed"))
        }
    }

    #[test]
    fn strings_print_raw_while_other_values_use_inspection_rendering() {
        assert_eq!(printable(&Value::String("hello".to_owned())), "hello");
        assert_eq!(printable(&Value::Int(7)), "7");
    }

    #[test]
    fn stream_failures_raise_structured_io_errors() {
        let span = Span::new(2, 4);
        let raised =
            match write_stream(FailingWriter, &Value::Int(1), false, "print", span).unwrap() {
                Err(raised) => raised,
                Ok(_) => panic!("failing stream should raise"),
            };
        assert_eq!(
            raised.value.render(),
            "{error=\"io_error\", operation=\"print\", message=\"closed\"}"
        );
        assert_eq!(raised.origin, span);

        let raised = match flush_stream(FailingWriter, "flush", span).unwrap() {
            Err(raised) => raised,
            Ok(_) => panic!("failing flush should raise"),
        };
        assert_eq!(
            raised.value.render(),
            "{error=\"io_error\", operation=\"flush\", message=\"closed\"}"
        );
    }

    #[test]
    fn automatic_flush_failures_use_the_print_operation() {
        for (newline, operation) in [(false, "print"), (true, "println")] {
            let raised = match write_stream(
                FlushFailingWriter,
                &Value::String("hello".to_owned()),
                newline,
                operation,
                Span::new(0, 0),
            )
            .unwrap()
            {
                Err(raised) => raised,
                Ok(_) => panic!("failing automatic flush should raise"),
            };
            assert_eq!(
                raised.value.render(),
                format!(
                    "{{error=\"io_error\", operation=\"{operation}\", message=\"flush failed\"}}"
                )
            );
        }
    }

    #[test]
    fn newline_output_appends_exactly_one_line_feed() {
        let mut output = Vec::new();
        write_stream(
            &mut output,
            &Value::String("hello".to_owned()),
            true,
            "println",
            Span::new(0, 0),
        )
        .unwrap()
        .unwrap();
        assert_eq!(output, b"hello\n");
    }
}
