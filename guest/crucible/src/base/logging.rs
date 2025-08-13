use std::{
    fmt::{self, Write as _},
    panic::{self, PanicHookInfo},
    sync::Once,
};

use crucible_abi as abi;
use tracing::{
    Event, Level, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{
    Layer,
    layer::{Context, SubscriberExt},
    registry::LookupSpan,
    util::SubscriberInitExt,
};
use wasmlink::{GuestStrRef, bind_port};

pub extern crate tracing;

// === Entry === //

bind_port! {
    fn [abi::LOG_MESSAGE] "crucible".log_message(abi::MessageLogArgs);
}

pub fn setup_logger() {
    static ONCE: Once = Once::new();

    ONCE.call_once(|| {
        panic::set_hook(Box::new(panic_hook));
        tracing_subscriber::registry().with(CrucibleLogLayer).init();

        // TODO
    });
}

// === Panicking === //

pub fn panic_hook(info: &PanicHookInfo<'_>) {
    log_message(&abi::MessageLogArgs {
        msg: GuestStrRef::new(&info.to_string()),
        file: GuestStrRef::new(info.location().map_or("<unknown>", |v| v.file())),
        module: GuestStrRef::new(""),
        line: info.location().map_or(0, |v| v.line()),
        column: info.location().map_or(0, |v| v.column()),
        level: abi::MessageLogLevel::Panic,
    });
}

// === Logging === //

// Adapted from https://github.com/old-storyai/tracing-wasm/blob/db1eb67e887307afb0014f66656d2d493e5b6187/src/lib.rs
#[derive(Debug, Default)]
pub struct CrucibleLogLayer;

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for CrucibleLogLayer {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::Id,
        ctx: Context<'_, S>,
    ) {
        let mut new_debug_record = StringRecorder::new();
        attrs.record(&mut new_debug_record);

        if let Some(span_ref) = ctx.span(id) {
            span_ref
                .extensions_mut()
                .insert::<StringRecorder>(new_debug_record);
        }
    }

    fn on_record(&self, id: &tracing::Id, values: &tracing::span::Record<'_>, ctx: Context<'_, S>) {
        if let Some(span_ref) = ctx.span(id) {
            if let Some(debug_record) = span_ref.extensions_mut().get_mut::<StringRecorder>() {
                values.record(debug_record);
            }
        }
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut recorder = StringRecorder::new();
        event.record(&mut recorder);

        let meta = event.metadata();

        log_message(&abi::MessageLogArgs {
            msg: GuestStrRef::new(&recorder.to_string()),
            file: GuestStrRef::new(meta.file().unwrap_or("")),
            module: GuestStrRef::new(meta.module_path().unwrap_or("")),
            line: meta.line().unwrap_or(0),
            column: 0,
            level: 'level: {
                let level = *meta.level();

                if level == Level::TRACE {
                    break 'level abi::MessageLogLevel::Trace;
                }

                if level == Level::DEBUG {
                    break 'level abi::MessageLogLevel::Debug;
                }

                if level == Level::INFO {
                    break 'level abi::MessageLogLevel::Info;
                }

                if level == Level::WARN {
                    break 'level abi::MessageLogLevel::Warn;
                }

                if level == Level::ERROR {
                    break 'level abi::MessageLogLevel::Error;
                }

                abi::MessageLogLevel::Info
            },
        });
    }
}

#[derive(Default)]
struct StringRecorder {
    display: String,
    is_following_args: bool,
}

impl StringRecorder {
    fn new() -> Self {
        StringRecorder {
            display: String::new(),
            is_following_args: false,
        }
    }
}

impl Visit for StringRecorder {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            if !self.display.is_empty() {
                self.display = format!("{:?}\n{}", value, self.display)
            } else {
                self.display = format!("{value:?}")
            }
        } else {
            if self.is_following_args {
                // following args
                writeln!(self.display).unwrap();
            } else {
                // first arg
                write!(self.display, " ").unwrap();
                self.is_following_args = true;
            }
            write!(self.display, "{} = {:?};", field.name(), value).unwrap();
        }
    }
}

impl fmt::Display for StringRecorder {
    fn fmt(&self, mut f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.display.is_empty() {
            write!(&mut f, " {}", self.display)
        } else {
            Ok(())
        }
    }
}
