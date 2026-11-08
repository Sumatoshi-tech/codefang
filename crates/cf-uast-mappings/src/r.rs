//! Mapping table for `r` — hand-maintained (the system of record).
//! Keep grammar-conformance, shape and Shotness language tests green when editing.

use cf_uast_mapping::{uast_language, LanguageMapping};

/// Mapping table for the `r` language.
pub static R: LanguageMapping = uast_language! {
    name: "r",
    extensions: [".r", ".R", ".rmd", ".Rmd"],
    rules: {
        program => {
            type: File,
        },
        call => {
            type: Call,
            roles: [Call],
        },
        arguments => {
            type: List,
            roles: [Argument],
        },
        argument => {
            type: Identifier,
            roles: [Argument],
        },
        // Named assignments own the Function identity and full span. Their
        // literal body is structural; other function values remain Lambdas.
        assigned_function_body ("(function_definition)") => {
            type: Synthetic,
            when: ["parent.operator == \"<-\"", "parent.rhs.type == \"function_definition\""],
        },
        assigned_body_equal ("(function_definition)") => {
            type: Synthetic,
            when: ["parent.operator == \"=\"", "parent.rhs.type == \"function_definition\""],
        },
        assigned_body_super_left ("(function_definition)") => {
            type: Synthetic,
            when: ["parent.operator == \"<<-\"", "parent.rhs.type == \"function_definition\""],
        },
        assigned_body_right ("(function_definition)") => {
            type: Synthetic,
            when: ["parent.type == \"parenthesized_expression\"", "parent.parent.operator == \"->\"", "parent.parent.lhs.body.type == \"function_definition\""],
        },
        assigned_body_super_right ("(function_definition)") => {
            type: Synthetic,
            when: ["parent.type == \"parenthesized_expression\"", "parent.parent.operator == \"->>\"", "parent.parent.lhs.body.type == \"function_definition\""],
        },
        function_definition => {
            type: Lambda,
            roles: [Lambda],
        },
        parameters => {
            type: Block,
            roles: [Parameter],
        },
        parameter => {
            type: Parameter,
            roles: [Parameter],
        },
        braced_expression => {
            type: Block,
            roles: [Body],
        },
        if_statement => {
            type: If,
            roles: [Branch],
        },
        for_statement => {
            type: Loop,
            roles: [Loop],
        },
        while_statement => {
            type: Loop,
            roles: [Loop],
        },
        repeat_statement => {
            type: Loop,
            roles: [Loop],
        },
        break => {
            type: Break,
            roles: [Break],
        },
        next => {
            type: Continue,
            roles: [Continue],
        },
        return => {
            type: Return,
            roles: [Return],
        },
        assigned_function ("(binary_operator lhs: (_) @declared_name)") => {
            type: Function, roles: [Function, Declaration],
            props: { "name": "@declared_name" },
            when: ["operator == \"<-\"", "rhs.type == \"function_definition\""],
        },
        assigned_equal ("(binary_operator lhs: (_) @declared_name)") => {
            type: Function, roles: [Function, Declaration],
            props: { "name": "@declared_name" },
            when: ["operator == \"=\"", "rhs.type == \"function_definition\""],
        },
        assigned_super_left ("(binary_operator lhs: (_) @declared_name)") => {
            type: Function, roles: [Function, Declaration],
            props: { "name": "@declared_name" },
            when: ["operator == \"<<-\"", "rhs.type == \"function_definition\""],
        },
        assigned_right ("(binary_operator rhs: (_) @declared_name)") => {
            type: Function, roles: [Function, Declaration],
            props: { "name": "@declared_name" },
            when: ["operator == \"->\"", "lhs.body.type == \"function_definition\""],
        },
        assigned_super_right ("(binary_operator rhs: (_) @declared_name)") => {
            type: Function, roles: [Function, Declaration],
            props: { "name": "@declared_name" },
            when: ["operator == \"->>\"", "lhs.body.type == \"function_definition\""],
        },
        binary_operator => {
            type: BinaryOp,
            roles: [Operator],
        },
        unary_operator => {
            type: UnaryOp,
            roles: [Operator],
        },
        identifier => {
            type: Identifier,
            roles: [Name],
        },
        float => {
            type: Literal,
            roles: [Literal],
        },
        integer => {
            type: Literal,
            roles: [Literal],
        },
        complex => {
            type: Literal,
            roles: [Literal],
        },
        string => {
            type: Literal,
            roles: [Literal],
        },
        string_content => {
            type: Literal,
            roles: [Literal],
        },
        true => {
            type: Literal,
            roles: [Literal],
        },
        false => {
            type: Literal,
            roles: [Literal],
        },
        null => {
            type: Literal,
            roles: [Literal],
        },
        na => {
            type: Literal,
            roles: [Literal],
        },
        inf => {
            type: Literal,
            roles: [Literal],
        },
        nan => {
            type: Literal,
            roles: [Literal],
        },
        comment => {
            type: Comment,
            roles: [Comment],
        },
        comma => {
            type: Identifier,
            roles: [Name],
        },
        subset => {
            type: Index,
            roles: [Index],
        },
        subset2 => {
            type: Index,
            roles: [Index],
        },
        extract_operator => {
            type: Field,
            roles: [Member],
        },
        formula => {
            type: BinaryOp,
            roles: [Operator],
        },
        special => {
            type: Identifier,
            roles: [Operator],
        },
    }
};
