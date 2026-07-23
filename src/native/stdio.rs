use std::io::{self, Write};

use crate::runtime::{NativeResult, Raised, RuntimeError, Value};
use crate::span::Span;

pub(crate) fn stdin_read_line(_: &[Value], span: Span) -> NativeResult {
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
        Err(error) => Ok(Err(Raised::io_error("read_line", error.to_string(), span))),
    }
}

pub(crate) fn io_print(args: &[Value], span: Span) -> NativeResult {
    write_stream(io::stdout(), &args[0], false, "print", span)
}

pub(crate) fn io_println(args: &[Value], span: Span) -> NativeResult {
    write_stream(io::stdout(), &args[0], true, "println", span)
}

pub(crate) fn io_eprint(args: &[Value], span: Span) -> NativeResult {
    write_stream(io::stderr(), &args[0], false, "eprint", span)
}

pub(crate) fn io_eprintln(args: &[Value], span: Span) -> NativeResult {
    write_stream(io::stderr(), &args[0], true, "eprintln", span)
}

fn write_stream(
    mut stream: impl Write,
    value: &Value,
    newline: bool,
    operation: &str,
    span: Span,
) -> NativeResult {
    let Value::String(rendered) = value else {
        return Err(RuntimeError::new(
            span,
            format!(
                "std/io.{operation} value must be a string, got {}",
                value.type_name()
            ),
        ));
    };
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
    fn output_requires_strings() {
        let error = match write_stream(Vec::new(), &Value::Int(7), false, "print", Span::new(0, 0))
        {
            Err(error) => error,
            Ok(_) => panic!("non-string output should be a hard diagnostic"),
        };
        assert_eq!(
            error.message,
            "std/io.print value must be a string, got integer"
        );
    }

    #[test]
    fn stream_failures_raise_structured_io_errors() {
        let span = Span::new(2, 4);
        let raised = match write_stream(
            FailingWriter,
            &Value::String("x".to_owned()),
            false,
            "print",
            span,
        )
        .unwrap()
        {
            Err(raised) => raised,
            Ok(_) => panic!("failing stream should raise"),
        };
        assert_eq!(
            raised.value.render(),
            "{error=\"io_error\", operation=\"print\", message=\"closed\"}"
        );
        assert_eq!(raised.origin, span);
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
