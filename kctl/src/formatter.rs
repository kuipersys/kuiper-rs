use serde_json::Value;

use crate::cmd::OutputFormat;

pub fn print_output(value: &Value, format: OutputFormat, pretty: bool) {
    match format {
        OutputFormat::Json => {
            if pretty {
                println!("{}", serde_json::to_string_pretty(value).unwrap());
            } else {
                println!("{}", serde_json::to_string(value).unwrap());
            }
        },

        OutputFormat::Yaml => {
            if pretty {
                println!("{}", serde_yml::to_string(value).unwrap());
            } else {
                println!("{}", serde_yml::to_string(value).unwrap());
            }
        },

        _ => {
            print_human_readable(value, 0);
        }
    }
}

fn print_human_readable(value: &Value, indent: usize) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                print!("{:indent$}{}: ", "", k, indent = indent);
                match v {
                    Value::Object(_) | Value::Array(_) => {
                        println!();
                        print_human_readable(v, indent + 2);
                    },
                    _ => println!("{}", v),
                }
            }
        },
        Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                print!("{:indent$}- ", "", indent = indent);
                match v {
                    Value::Object(_) | Value::Array(_) => {
                        println!();
                        print_human_readable(v, indent + 2);
                    },
                    _ => println!("{}", v),
                }
            }
        },
        _ => println!("{:indent$}{}", "", value, indent = indent),
    }
}