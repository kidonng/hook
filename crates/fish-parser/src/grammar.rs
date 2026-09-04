use crate::ast::*;
use crate::line_index::LineIndex;
use crate::unescape::*;

#[derive(Debug)]
enum CommandItem {
    Arg(Word),
    Redir(Redirection),
}

peg::parser! {
    pub grammar fish_grammar(line_index: &LineIndex) for str {
        pub rule program() -> Program
            = statement_sep()? shebang:shebang_line()? statements:statement_list() _* ![_] {
                Program { shebang, statements }
            }

        rule shebang_line() -> String
            = "#!" s:$( [^'\n']* ) ("\n" / ![_]) {
                format!("#!{}", s)
            }

        pub rule statement_list() -> Vec<Statement>
            = statement_sep()? stmts:(s:statement() ** statement_sep()) statement_sep()? {
                stmts.into_iter().flatten().filter(|s| match &s.kind {
                    StatementKind::Comment(_) => true,
                    StatementKind::Pipeline(p) => !p.elements.is_empty(),
                    _ => true,
                }).collect()
            }
        rule statement_sep()
            = (_* ['\n' | ';'])+ _*

        rule _() = [' ' | '\t']

        rule ident() -> String
            = s:$(['a'..='z' | 'A'..='Z' | '_']['a'..='z' | 'A'..='Z' | '0'..='9' | '_']*) { s.to_string() }

        rule keyword_char() = ['a'..='z' | 'A'..='Z' | '0'..='9' | '_']

        rule reserved_keyword()
            = ("if" / "else" / "switch" / "case" / "for" / "while" / "function" / "begin" / "end") !keyword_char()

        rule block_terminator()
            = ("end" / "else" / "case") !keyword_char()
            / "}"
        pub rule statement() -> Vec<Statement>
            = !block_terminator() start:position!() s:inner_statement() end:position!() {
                let span = SourceSpan::new(line_index.line_of(start), line_index.line_of(end));
                s.into_iter().map(|kind| Statement::new(kind, span)).collect()
            }

        rule inner_statement() -> Vec<StatementKind>
            = s:comment() { vec![s] }
            / s:return_stmt() { vec![s] }
            / s:break_stmt() { vec![s] }
            / s:continue_stmt() { vec![s] }
            / pipeline_stmt()
            / s:if_stmt() { vec![s] }
            / s:switch_stmt() { vec![s] }
            / s:for_stmt() { vec![s] }
            / s:while_stmt() { vec![s] }
            / s:function_stmt() { vec![s] }
            / s:begin_stmt() { vec![s] }
            / s:compound_block_stmt() { vec![s] }
        rule comment() -> StatementKind
            = "#" s:$((!['\n'][_])*) {
                StatementKind::Comment(s.to_string())
            }
        rule return_stmt() -> StatementKind
            = "return" !keyword_char() _+ w:word() { StatementKind::Return(Some(w)) }
            / "return" !keyword_char() { StatementKind::Return(None) }

        rule break_stmt() -> StatementKind
            = "break" !keyword_char() { StatementKind::Break }

        rule continue_stmt() -> StatementKind
            = "continue" !keyword_char() { StatementKind::Continue }
        rule pipeline_stmt() -> Vec<StatementKind>
            = chain:pipeline_chain() {
                chain.into_iter().map(StatementKind::Pipeline).collect()
            }
        rule pipeline_chain() -> Vec<Pipeline>
            = head:pipeline() tail:(cont_space() comb:combinator_op() cont_space() p:pipeline() { (comb, p) })* {
                let mut list = vec![head];
                for (comb, mut p) in tail {
                    p.combinator = comb;
                    list.push(p);
                }
                list
            }

        rule combinator_op() -> Combinator
            = "and" !keyword_char() { Combinator::And }
            / "or" !keyword_char() { Combinator::Or }
            / "&&" { Combinator::And }
            / "||" { Combinator::Or }

        rule cont_space()
            = (_ / ("\\" "\n") / ("#" [^'\n']* "\n") / "\n")*

        rule pipe_op() -> PipeOperator
            = ("&|" / "|&") { PipeOperator::StdoutAndStderr }
            / fd:(n:$(['0'..='9']+) { n.parse::<u32>().unwrap() })? ">|" {
                PipeOperator::Fd(fd.unwrap_or(1))
            }
            / "|" { PipeOperator::Stdout }

        rule pipe_sep() -> PipeOperator
            = _* p:pipe_op() cont_space() { p }

        rule negate() -> bool
            = ("not" !keyword_char() _+ / "!" _*) { true }

        rule bg_separator()
            = &[' ' | '\t' | '\n' | ';' | '<' | '>' | '&' | '|'] / ![_]

        rule bg_flag() -> bool
            = _* "&" !['&' | '>'] bg_separator() { true }

        rule block_element() -> Statement
            = start:position!() s:(
                if_stmt()
                / switch_stmt()
                / for_stmt()
                / while_stmt()
                / begin_stmt()
                / compound_block_stmt()
            ) end:position!() {
                let span = SourceSpan::new(line_index.line_of(start), line_index.line_of(end));
                Statement::new(s, span)
            }
        rule pipeline_element() -> PipelineElement
            = b:block_element() { PipelineElement::Block(b) }
            / c:command() { PipelineElement::Command(c) }

        rule pipeline() -> Pipeline
            = !block_terminator() comb:combinator_prefix()? _* negate:negate()? head:block_element() tail:(sep:pipe_sep() el:pipeline_element() { (sep, el) })+ bg:bg_flag()? {
                let mut elements = vec![PipelineElement::Block(head)];
                let mut pipe_operators = Vec::new();
                for (op, next_el) in tail {
                    pipe_operators.push(op);
                    elements.push(next_el);
                }
                Pipeline {
                    negate: negate.unwrap_or(false),
                    elements,
                    pipe_operators,
                    combinator: comb.unwrap_or(Combinator::None),
                    background: bg.is_some(),
                }
            }
            / !block_terminator() comb:combinator_prefix()? _* negate:negate()? head:command() tail:(sep:pipe_sep() el:pipeline_element() { (sep, el) })* bg:bg_flag()? {
                let mut elements = vec![PipelineElement::Command(head)];
                let mut pipe_operators = Vec::new();
                for (op, next_el) in tail {
                    pipe_operators.push(op);
                    elements.push(next_el);
                }
                Pipeline {
                    negate: negate.unwrap_or(false),
                    elements,
                    pipe_operators,
                    combinator: comb.unwrap_or(Combinator::None),
                    background: bg.is_some(),
                }
            }

        rule combinator_prefix() -> Combinator
            = "and" !keyword_char() _+ { Combinator::And }
            / "or" !keyword_char() _+ { Combinator::Or }
            / "&&" _* { Combinator::And }
            / "||" _* { Combinator::Or }

        rule line_cont()
            = "\\" _* "\n" (_* "#" [^'\n']* "\n")* _*

        rule cmd_arg_sep()
            = (_ / line_cont())+

        rule variable_assignment() -> VariableAssignment
            = name:var_name() "=" val:word() {
                VariableAssignment { name, value: val }
            }

        rule command() -> Command
            = !reserved_keyword() assigns:(a:variable_assignment() ++ cmd_arg_sep()) rest:(cmd_arg_sep() items:(command_item() ++ cmd_arg_sep()) { items })? {
                let mut args = Vec::new();
                let mut redirections = Vec::new();
                if let Some(items) = rest {
                    for item in items {
                        match item {
                            CommandItem::Arg(w) => args.push(w),
                            CommandItem::Redir(r) => redirections.push(r),
                        }
                    }
                }
                Command {
                    assignments: assigns,
                    args,
                    redirections,
                }
            }
            / !reserved_keyword() items:(command_item() ++ cmd_arg_sep()) {
                let mut args = Vec::new();
                let mut redirections = Vec::new();
                for item in items {
                    match item {
                        CommandItem::Arg(w) => args.push(w),
                        CommandItem::Redir(r) => redirections.push(r),
                    }
                }
                Command {
                    assignments: vec![],
                    args,
                    redirections,
                }
            }

        rule command_item() -> CommandItem
            = r:redirection() { CommandItem::Redir(r) }
            / !( (['0'..='9']*) ">|" ) w:word() { CommandItem::Arg(w) }

        rule redirection() -> Redirection
            = fd:(n:$(['0'..='9']+) { n.parse::<u32>().unwrap() })? mode:redirect_mode() _* target:word() {
                Redirection { fd, mode, target }
            }

        rule redirect_mode() -> RedirectMode
            = ">>&" { RedirectMode::DupOutput }
            / ">>?" { RedirectMode::NoClobberAppend }
            / ">>" { RedirectMode::Append }
            / ">&" { RedirectMode::DupOutput }
            / "<&" { RedirectMode::DupInput }
            / "<?" { RedirectMode::SafeInput }
            / ">?" { RedirectMode::NoClobberOutput }
            / ">" { RedirectMode::Output }
            / "<" { RedirectMode::Input }
            / "^^" { RedirectMode::AppendAndErr }
            / "^" { RedirectMode::OutputAndErr }
            / "&>>?" { RedirectMode::NoClobberAppendAndErr }
            / "&>>" { RedirectMode::AppendAndErr }
            / "&>?" { RedirectMode::NoClobberOutputAndErr }
            / "&>" { RedirectMode::OutputAndErr }
        rule word() -> Word
            = parts:(word_part()+ ) {
                Word { parts }
            }

        rule word_part() -> WordPart
            = single_quoted()
            / double_quoted()
            / variable_ref()
            / command_subst()
            / brace_expansion()
            / literal()

        rule literal() -> WordPart
            = s:$( ( [^' ' | '\t' | '\n' | ';' | '|' | '&' | '<' | '>' | '^' | '(' | ')' | '{' | '}' | '$' | '\'' | '\"' | '#'] / ("&" !bg_separator()) / ("\\" [_]) )+ ) {
                WordPart::Literal(unescape(s))
            }

        rule single_quoted() -> WordPart
            = "'" s:$(( "\\'" / "\\\\" / [^'\''] )*) "'" {
                WordPart::SingleQuoted(unescape_single_quoted(s))
            }

        rule double_quoted() -> WordPart
            = "\"" parts:double_quoted_part()* "\"" {
                WordPart::DoubleQuoted(parts)
            }

        rule double_quoted_part() -> WordPart
            = variable_ref()
            / dollar_command_subst()
            / s:$(( [^'\"' | '$' | '\\'] / ("\\" [_]) )+) {
                WordPart::Literal(unescape_double_quoted(s))
            }

        rule var_name() -> String
            = s:$(['a'..='z' | 'A'..='Z' | '0'..='9' | '_']+) { s.to_string() }

        rule variable_ref() -> WordPart
            = "$" inner:variable_ref() slices:slices() {
                if let WordPart::Variable(vref) = inner {
                    WordPart::Variable(VariableRef {
                        target: VariableTarget::Indirect(Box::new(vref)),
                        slices,
                    })
                } else {
                    unreachable!()
                }
            }
            / "$" name:var_name() slices:slices() {
                WordPart::Variable(VariableRef {
                    target: VariableTarget::Named(name),
                    slices,
                })
            }

        rule slice_item() -> Slice
            = start:slice_index()? _* ".." _* end:slice_index()? {
                Slice::Range { start, end }
            }
            / idx:slice_index() {
                Slice::Index(idx)
            }

        rule slice_bracket() -> Vec<Slice>
            = "[" _* items:(slice_item() ++ (_+)) _* "]" {
                items
            }

        rule slices() -> Vec<Slice>
            = brackets:slice_bracket()* {
                brackets.into_iter().flatten().collect()
            }

        rule slice_index() -> SliceIndex
            = "-" n:$(['0'..='9']+) { SliceIndex::Neg(n.parse::<usize>().unwrap()) }
            / n:$(['0'..='9']+) { SliceIndex::Pos(n.parse::<usize>().unwrap()) }
            / v:variable_ref() {
                if let WordPart::Variable(vref) = v {
                    SliceIndex::Variable(vref)
                } else {
                    unreachable!()
                }
            }
        rule dollar_command_subst() -> WordPart
            = "$(" _* stmts:statement_list() _* ")" slices:slices() {
                WordPart::CommandSubst { statements: stmts, slices }
            }

        rule command_subst() -> WordPart
            = dollar_command_subst()
            / "(" _* stmts:statement_list() _* ")" slices:slices() {
                WordPart::CommandSubst { statements: stmts, slices }
            }
        rule brace_literal() -> WordPart
            = s:$( ( [^' ' | '\t' | '\n' | ';' | '|' | '&' | '<' | '>' | '^' | '(' | ')' | '{' | '}' | ',' | '$' | '\'' | '\"' | '#'] / ("\\" [_]) )+ ) {
                WordPart::Literal(unescape(s))
            }

        rule brace_part() -> WordPart
            = single_quoted()
            / double_quoted()
            / variable_ref()
            / command_subst()
            / brace_expansion()
            / brace_literal()

        rule brace_item() -> Word
            = parts:(brace_part()* ) {
                Word { parts }
            }

        rule brace_expansion() -> WordPart
            = "{" _* items:(brace_item() ** (_* "," _*)) _* "}" {
                let has_comma = items.len() > 1;
                let has_var = items.iter().any(|w| w.parts.iter().any(|p| matches!(p, WordPart::Variable(_))));
                if has_comma || has_var {
                    WordPart::BraceExpansion(items)
                } else {
                    let inner = if items.is_empty() {
                        String::new()
                    } else {
                        items.iter().map(|w| {
                            w.parts.iter().map(|p| match p {
                                WordPart::Literal(s) => s.clone(),
                                WordPart::SingleQuoted(s) => format!("'{}'", s),
                                _ => String::new(),
                            }).collect::<String>()
                        }).collect::<Vec<_>>().join(",")
                    };
                    WordPart::Literal(format!("{{{}}}", inner))
                }
            }

        rule if_stmt() -> StatementKind
            = "if" !keyword_char() _+ cond:pipeline_chain() statement_sep()
              then_body:statement_list()
              elifs:elif_branch()*
              else_body:else_branch()?
              "end" !keyword_char() redirs:(_* r:redirection() { r })* {
                StatementKind::If(IfStatement {
                    condition: cond,
                    then_body,
                    elif_branches: elifs,
                    else_body,
                    redirections: redirs,
                })
            }

        rule elif_branch() -> (Vec<Pipeline>, Vec<Statement>)
            = "else" !keyword_char() _+ "if" !keyword_char() _+ cond:pipeline_chain() statement_sep() body:statement_list() {
                (cond, body)
            }

        rule else_branch() -> Vec<Statement>
            = "else" !keyword_char() statement_sep() body:statement_list() {
                body
            }

        rule switch_stmt() -> StatementKind
            = "switch" !keyword_char() _+ val:word() statement_sep()
              cases:case_clause()*
              "end" !keyword_char() redirs:(_* r:redirection() { r })* {
                StatementKind::Switch(SwitchStatement {
                    value: val,
                    cases,
                    redirections: redirs,
                })
            }

        rule case_clause() -> CaseClause
            = "case" !keyword_char() _+ pats:(word() ++ (_+)) statement_sep()
              body:statement_list() {
                CaseClause { patterns: pats, body }
            }

        rule for_stmt() -> StatementKind
            = "for" !keyword_char() _+ var:var_name() _+ "in" !keyword_char() vals:(_+ w:word() { w })* statement_sep()
              body:statement_list()
              "end" !keyword_char() redirs:(_* r:redirection() { r })* {
                StatementKind::For(ForStatement {
                    variable: var,
                    values: vals,
                    body,
                    redirections: redirs,
                })
            }

        rule while_stmt() -> StatementKind
            = "while" !keyword_char() _+ cond:pipeline_chain() statement_sep()
              body:statement_list()
              "end" !keyword_char() redirs:(_* r:redirection() { r })* {
                StatementKind::While(WhileStatement {
                    condition: cond,
                    body,
                    redirections: redirs,
                })
            }

        rule function_stmt() -> StatementKind
            = "function" !keyword_char() _+ name:word() options:(_+ w:word() { w })* statement_sep()
              body:statement_list()
              "end" !keyword_char() {
                StatementKind::Function(FunctionStatement {
                    name,
                    options,
                    body,
                })
            }
        rule begin_stmt() -> StatementKind
            = comb:combinator_prefix()? _* "begin" !keyword_char() statement_sep()
              body:statement_list()
              "end" !keyword_char() redirs:(_* r:redirection() { r })* {
                StatementKind::BeginBlock(BeginBlock {
                    combinator: comb.unwrap_or(Combinator::None),
                    body,
                    redirections: redirs,
                })
            }
        rule compound_block_stmt() -> StatementKind
            = comb:combinator_prefix()? _* "{" (_* ['\n' | ';'])* _*
              body:statement_list()
              _* "}" redirs:(_* r:redirection() { r })* {
                StatementKind::BeginBlock(BeginBlock {
                    combinator: comb.unwrap_or(Combinator::None),
                    body,
                    redirections: redirs,
                })
            }
    }
}

pub fn parse(input: &str) -> Result<Program, peg::error::ParseError<peg::str::LineCol>> {
    let clean_input = input.trim_start_matches('\u{feff}');
    let line_index = LineIndex::from_source(clean_input);
    fish_grammar::program(clean_input, &line_index)
}
