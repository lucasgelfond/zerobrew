use std::path::Path;
use tree_sitter::{Node, Parser};
use zb_core::{Error, Formula};

pub struct FormulaParser;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum CpuArch {
    X86_64,
    ARM64,
}

fn get_current_arch() -> CpuArch {
    #[cfg(target_arch = "x86_64")]
    {
        CpuArch::X86_64
    }
    #[cfg(target_arch = "aarch64")]
    {
        CpuArch::ARM64
    }
}

impl FormulaParser {
    pub fn parse_file(path: &Path, name: &str) -> Result<Formula, Error> {
        let content = std::fs::read_to_string(path).map_err(|e| Error::IoError(e.to_string()))?;
        Self::parse(&content, name)
    }

    pub fn parse(source: &str, name: &str) -> Result<Formula, Error> {
        let mut parser = Parser::new();
        parser
            .set_language(tree_sitter_ruby::language())
            .expect("Error loading Ruby grammar");

        let tree = parser.parse(source, None).ok_or_else(|| Error::ParseError {
            message: "Failed to parse Ruby source".to_string(),
        })?;

        let mut formula = Formula {
            name: name.to_string(),
            ..Default::default()
        };

        let root_node = tree.root_node();
        let mut context = ParsingContext::default();
        visit_node(root_node, source, &mut formula, &mut context);

        Ok(formula)
    }
}

#[derive(Default, Clone)]
struct ParsingContext {
    in_on_linux: bool,
    in_on_macos: bool,
    in_on_intel: bool,
    in_on_arm: bool,
}

fn visit_node(node: Node, source: &str, data: &mut Formula, context: &mut ParsingContext) {
    let kind = node.kind();

    match kind {
        "call" => {
            let method_node = node.child_by_field_name("method").unwrap();
            let method_name = method_node.utf8_text(source.as_bytes()).unwrap_or("");

            match method_name {
                "version" => {
                    if let Some(v) = find_string_argument(&node, source) {
                        data.versions.stable = v;
                    }
                }
                "url" => {
                    if let Some(u) = find_string_argument(&node, source) {
                        process_url(u, data, context);
                    }
                }
                "sha256" => {
                    if let Some(s) = find_string_argument(&node, source) {
                        process_sha256(s, data, context);
                    }
                }
                "depends_on" => {
                    if let Some(d) = find_string_argument(&node, source) {
                        data.dependencies.push(d);
                    }
                }
                "on_linux" => {
                    let mut new_ctx = context.clone();
                    new_ctx.in_on_linux = true;
                    if let Some(block) = node.child_by_field_name("block") {
                        process_block(block, source, data, &mut new_ctx);
                    }
                }
                "on_macos" | "on_macos_intel" | "on_arm" | "on_intel" => {
                    let mut new_ctx = context.clone();
                    if method_name == "on_macos" || method_name == "on_macos_intel" {
                        new_ctx.in_on_macos = true;
                    }
                    if method_name == "on_intel" || method_name == "on_macos_intel" {
                        new_ctx.in_on_intel = true;
                    }
                    if method_name == "on_arm" {
                        new_ctx.in_on_arm = true;
                    }
                    if let Some(block) = node.child_by_field_name("block") {
                        process_block(block, source, data, &mut new_ctx);
                    }
                }
                "bottle" => {
                    if let Some(block) = node.child_by_field_name("block") {
                        process_bottle_block(block, source, data);
                    }
                }
                _ => {}
            }
        }
        "if" => {
            process_conditional(node, source, data, context);
        }
        _ => {
            for i in 0..node.child_count() {
                visit_node(node.child(i).unwrap(), source, data, context);
            }
        }
    }
}

fn process_block(node: Node, source: &str, data: &mut Formula, context: &mut ParsingContext) {
    if kind_is_body(node.kind()) {
        for i in 0..node.child_count() {
            visit_node(node.child(i).unwrap(), source, data, context);
        }
    } else {
        for i in 0..node.child_count() {
            process_block(node.child(i).unwrap(), source, data, context);
        }
    }
}

fn kind_is_body(kind: &str) -> bool {
    kind == "block_body" || kind == "do_block" || kind == "body_statement" || kind == "program"
}

fn process_url(url: String, data: &mut Formula, context: &ParsingContext) {
    // For now, if we are in on_linux, we might want to store this URL if we don't have a bottle
    // But Zerobrew primarily uses bottles. If no bottle is found, it will try to use this URL.
    // We'll store it in a special "fake" bottle entry for the current platform if we are in a matching block.
    
    let current_os_matches = (context.in_on_linux && cfg!(target_os = "linux")) ||
                             (context.in_on_macos && cfg!(target_os = "macos")) ||
                             (!context.in_on_linux && !context.in_on_macos);

    let current_arch_matches = (context.in_on_intel && get_current_arch() == CpuArch::X86_64) ||
                              (context.in_on_arm && get_current_arch() == CpuArch::ARM64) ||
                              (!context.in_on_intel && !context.in_on_arm);

    if current_os_matches && current_arch_matches {
        let tag = match (cfg!(target_os = "linux"), get_current_arch()) {
            (true, CpuArch::X86_64) => "x86_64_linux",
            (true, CpuArch::ARM64) => "aarch64_linux",
            (false, CpuArch::X86_64) => "monterey", // Default for now
            (false, CpuArch::ARM64) => "arm64_monterey",
        };
        
        data.bottle.stable.files.entry(tag.to_string()).or_default().url = url;
    }
}

fn process_sha256(sha: String, data: &mut Formula, context: &ParsingContext) {
    let current_os_matches = (context.in_on_linux && cfg!(target_os = "linux")) ||
                             (context.in_on_macos && cfg!(target_os = "macos")) ||
                             (!context.in_on_linux && !context.in_on_macos);

    let current_arch_matches = (context.in_on_intel && get_current_arch() == CpuArch::X86_64) ||
                              (context.in_on_arm && get_current_arch() == CpuArch::ARM64) ||
                              (!context.in_on_intel && !context.in_on_arm);

    if current_os_matches && current_arch_matches {
        let tag = match (cfg!(target_os = "linux"), get_current_arch()) {
            (true, CpuArch::X86_64) => "x86_64_linux",
            (true, CpuArch::ARM64) => "aarch64_linux",
            (false, CpuArch::X86_64) => "monterey",
            (false, CpuArch::ARM64) => "arm64_monterey",
        };
        
        data.bottle.stable.files.entry(tag.to_string()).or_default().sha256 = sha;
    }
}

fn process_conditional(node: Node, source: &str, data: &mut Formula, context: &mut ParsingContext) {
    // Simplified if/elsif/else handling
    // We check the condition, if it matches our platform, we process the body.
    
    let mut current_node = Some(node);
    while let Some(n) = current_node {
        let kind = n.kind();
        if kind == "if" || kind == "elsif" {
            if let Some(condition) = n.child_by_field_name("condition") {
                if should_process_condition_node(&condition, source) {
                    if let Some(body) = n.child_by_field_name("consequence") {
                        process_branch_body(body, source, data, context);
                    }
                    return; // Handled
                }
            }
            // Move to alternative
            current_node = n.child_by_field_name("alternative");
        } else if kind == "else" {
            // If we reached else, it means no if/elsif matched
            if let Some(body) = n.child_by_field_name("consequence") {
               process_branch_body(body, source, data, context);
            }
            return;
        } else {
            break;
        }
    }
}

fn process_branch_body(node: Node, source: &str, data: &mut Formula, context: &mut ParsingContext) {
    // Use visit_node but avoid visiting the if structure again recursively from inside itself incorrectly
    // Actually we just want to visit the children of the body
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        // Skip some nodes that might cause infinite recursion or double processing if not careful
        // In Ruby tree-sitter, the body might contain statements directly.
        visit_node(child, source, data, context);
    }
}

fn process_bottle_block(node: Node, source: &str, data: &mut Formula) {
    // Scan for sha256 keyword arguments
    // e.g., sha256 arm64_sonoma: "..."
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() == "call" {
            let method_node = child.child_by_field_name("method").unwrap();
            let method_name = method_node.utf8_text(source.as_bytes()).unwrap_or("");
            if method_name == "sha256" {
                if let Some((tag, sha)) = find_platform_keyword_argument(&child, source) {
                    data.bottle.stable.files.entry(tag).or_default().sha256 = sha;
                }
            }
        }
        process_bottle_block(child, source, data);
    }
}

fn should_process_condition_node(node: &Node, source: &str) -> bool {
    let condition_text = node.utf8_text(source.as_bytes()).unwrap_or("");
    should_process_conditional_text(condition_text)
}

fn should_process_conditional_text(text: &str) -> bool {
    let is_macos = cfg!(target_os = "macos");
    let is_linux = cfg!(target_os = "linux");
    let current_arch = get_current_arch();

    for part in text.split("&&") {
        let part = part.trim();
        if part.contains("OS.mac?") && !is_macos { return false; }
        if part.contains("OS.linux?") && !is_linux { return false; }
        if (part.contains("Hardware::CPU.intel?") || part.contains("Hardware::CPU.is_intel?")) && current_arch != CpuArch::X86_64 { return false; }
        if (part.contains("Hardware::CPU.arm?") || part.contains("Hardware::CPU.is_arm?")) && current_arch != CpuArch::ARM64 { return false; }
    }
    true
}

#[allow(dead_code)]
fn find_keyword_argument(node: &Node, source: &str, name: &str) -> Option<String> {
    if let Some(args) = node.child_by_field_name("arguments") {
        for i in 0..args.child_count() {
            let child = args.child(i).unwrap();
            let kind = child.kind();
            if kind == "keyword_argument" || kind == "pair" {
                if let Some(name_node) = child.child_by_field_name("name").or_else(|| child.child_by_field_name("key")) {
                    let key = name_node.utf8_text(source.as_bytes()).unwrap_or("");
                    let key = key.trim_end_matches(':');
                    if key == name {
                        if let Some(value_node) = child.child_by_field_name("value") {
                            let text = value_node.utf8_text(source.as_bytes()).unwrap_or("");
                            return Some(text.trim_matches('"').trim_matches('\'').to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

fn find_platform_keyword_argument(node: &Node, source: &str) -> Option<(String, String)> {
    if let Some(args) = node.child_by_field_name("arguments") {
        for i in 0..args.child_count() {
            let child = args.child(i).unwrap();
            let kind = child.kind();
            if kind == "keyword_argument" || kind == "pair" {
                if let Some(name_node) = child.child_by_field_name("name").or_else(|| child.child_by_field_name("key")) {
                    let name = name_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                    let name = name.trim_end_matches(':').to_string();
                    if name == "cellar" || name == "tag" || name == "revision" || name == "branch" || name == "using" {
                        continue;
                    }
                    if let Some(value_node) = child.child_by_field_name("value") {
                        let value = value_node.utf8_text(source.as_bytes()).unwrap_or("");
                        return Some((name, value.trim_matches('"').trim_matches('\'').to_string()));
                    }
                }
            }
        }
    }
    None
}

fn find_string_argument(node: &Node, source: &str) -> Option<String> {
    if let Some(args) = node.child_by_field_name("arguments") {
        for i in 0..args.child_count() {
            if let Some(child) = args.child(i) {
                if child.kind() == "string" || child.kind() == "simple_string" {
                    let text = child.utf8_text(source.as_bytes()).ok()?;
                    return Some(text.trim_matches('"').trim_matches('\'').to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_formula() {
        let content = r#"
class TestFormula < Formula
  version "1.0.0"
  depends_on "foo"
  
  on_linux do
    url "https://example.com/test-linux.tar.gz"
    sha256 "abc123"
  end
end
"#;

        let formula = FormulaParser::parse(content, "test").unwrap();
        assert_eq!(formula.name, "test");
        assert_eq!(formula.versions.stable, "1.0.0");
        assert_eq!(formula.dependencies, vec!["foo"]);
        #[cfg(target_os = "linux")]
        assert!(formula.bottle.stable.files.contains_key("x86_64_linux") || formula.bottle.stable.files.contains_key("aarch64_linux"));
    }
}
