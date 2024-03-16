use deno_core::{anyhow::Error, error::generic_error};

use crate::utils::fs::atomic_write;

use tokio::fs;
use tokio::task::JoinSet;

use dprint_plugin_json;
use dprint_plugin_markdown::configuration::TextWrap;
use dprint_plugin_typescript::configuration::{QuoteProps, SortOrder};

use glob::glob;

use std::ffi::OsStr;
use std::path::Path;

// TODO(lino-levan): Make typescript/json/markdown global static variables when `LazyCell` is stable.
// https://doc.rust-lang.org/std/cell/struct.LazyCell.html

fn format_typescript_file(path: &Path, contents: &str) -> Result<Option<String>, Error> {
    dprint_plugin_typescript::format_text(
        path,
        contents,
        &dprint_plugin_typescript::configuration::ConfigurationBuilder::new()
            .deno()
            .use_tabs(true)
            .quote_props(QuoteProps::AsNeeded)
            .comment_line_force_space_after_slashes(true)
            .ignore_node_comment_text("bueno-fmt-ignore")
            .ignore_file_comment_text("bueno-fmt-ignore-file")
            .module_sort_import_declarations(SortOrder::CaseInsensitive)
            .module_sort_export_declarations(SortOrder::CaseInsensitive)
            .build(),
    )
}

fn format_json_file(path: &Path, contents: &str) -> Result<Option<String>, Error> {
    dprint_plugin_json::format_text(
        path,
        contents,
        &dprint_plugin_json::configuration::ConfigurationBuilder::new()
            .line_width(80)
            .use_tabs(true)
            .ignore_node_comment_text("bueno-fmt-ignore")
            .comment_line_force_space_after_slashes(true)
            .build(),
    )
}

fn format_markdown_file(path: &Path, contents: &str) -> Result<Option<String>, Error> {
    dprint_plugin_markdown::format_text(
        &contents,
        &dprint_plugin_markdown::configuration::ConfigurationBuilder::new()
            .text_wrap(TextWrap::Always)
            .ignore_directive("bueno-fmt-ignore")
            .ignore_start_directive("bueno-fmt-ignore-start")
            .ignore_end_directive("bueno-fmt-ignore-end")
            .ignore_file_directive("bueno-fmt-ignore-file")
            .build(),
        |tag, text, _line_number| format_file(path, tag, text),
    )
}

fn format_file(path: &Path, ext: &str, contents: &str) -> Result<Option<String>, Error> {
    match ext {
        "js" | "ts" | "jsx" | "tsx" => format_typescript_file(path, contents),
        "json" | "jsonc" => format_json_file(path, contents),
        "md" | "markdown" => format_markdown_file(path, contents),
        _ => Ok(None),
    }
}

pub struct FormatOptions<'a> {
    pub check: bool,
    pub glob: &'a String,
}

impl<'a> FormatOptions<'a> {
    pub fn new(check: bool, glob: &'a String) -> Self {
        Self { check, glob }
    }
}

pub async fn fmt(options: FormatOptions<'_>) -> Result<(), Error> {
    let paths: Result<Vec<_>, _> = glob(&options.glob)?.into_iter().collect();

    let mut joinset: JoinSet<Result<_, Error>> = JoinSet::new();

    for path in paths? {
        let ext = match path.extension().and_then(OsStr::to_str) {
            Some(ext @ ("js" | "ts" | "jsx" | "tsx" | "json" | "jsonc" | "md" | "markdown")) => {
                ext.to_string()
            }
            _ => continue,
        };

        joinset.spawn(async move {
            let contents = fs::read_to_string(&path).await?;

            if let Some(formatted) = format_file(&path, &ext, &contents)? {
                println!("Formatted: {}", path.display());
                if !options.check {
                    atomic_write(&path, formatted).await?;
                    println!("Wrote: {:?}", path);
                }
            }
            Ok(())
        });
    }

    while let Some(Ok(res)) = joinset.join_next().await {
        res.map_err(|e| generic_error(format!("Formatter failed: {:?}", e)))?;
    }

    Ok(())
}
