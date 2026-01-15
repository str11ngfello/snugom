use crate::commands::{init, migrate, schema};

#[derive(Clone, Copy)]
pub struct ExampleGroup {
    pub title: &'static str,
    pub commands: &'static [&'static str],
}

#[derive(Clone, Copy)]
pub struct CommandExample {
    pub name: &'static str,
    pub groups: &'static [ExampleGroup],
}

pub fn command_examples() -> &'static [CommandExample] {
    &[
        CommandExample {
            name: "init",
            groups: init::EXAMPLES,
        },
        CommandExample {
            name: "migrate",
            groups: migrate::EXAMPLES,
        },
        CommandExample {
            name: "schema",
            groups: schema::EXAMPLES,
        },
    ]
}
