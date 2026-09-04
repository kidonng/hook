use crate::bash::ir::LoweredWord;
use fish_parser::ast::{Word, WordPart};

#[derive(Default, Debug, Clone)]
pub struct WordsBuilder {
    words: Vec<LoweredWord>,
}

impl WordsBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, word: impl Into<LoweredWord>) -> &mut Self {
        self.words.push(word.into());
        self
    }

    pub fn push_words(&mut self, text: &str) -> &mut Self {
        for part in text.split_whitespace() {
            self.words.push(LoweredWord::from_literal(part));
        }
        self
    }

    pub fn extend<I>(&mut self, iter: I) -> &mut Self
    where
        I: IntoIterator<Item = LoweredWord>,
    {
        self.words.extend(iter);
        self
    }

    pub fn into_vec(self) -> Vec<LoweredWord> {
        self.words
    }
}

pub fn extract_single_variable(word: &Word) -> Option<&str> {
    if word.parts.len() == 1 {
        match &word.parts[0] {
            WordPart::Variable(vref) if vref.slices.is_empty() => vref.name(),
            WordPart::DoubleQuoted(inner) if inner.len() == 1 => {
                if let WordPart::Variable(vref) = &inner[0] {
                    if vref.slices.is_empty() {
                        return vref.name();
                    }
                }
                None
            }
            _ => None,
        }
    } else {
        None
    }
}

pub fn extract_function_meta(options: &[Word]) -> (Vec<String>, Option<String>) {
    let mut named_args = Vec::new();
    let mut description = None;

    let mut i = 0;
    while i < options.len() {
        let opt = &options[i];
        if let Some(lit) = opt.as_single_literal() {
            match lit {
                "-a" | "--argument-names" => {
                    i += 1;
                    while i < options.len() {
                        let next = &options[i];
                        if let Some(name) = next.as_single_literal() {
                            if !name.starts_with('-') {
                                named_args.push(name.to_string());
                                i += 1;
                                continue;
                            }
                        }
                        break;
                    }
                    continue;
                }
                "-d" | "--description" => {
                    i += 1;
                    if i < options.len() {
                        description = extract_word_string(&options[i]);
                        i += 1;
                    }
                    continue;
                }
                "-w" | "--wraps" | "-V" | "--inherit-variable" | "-e" | "--on-event" | "-s"
                | "--on-signal" | "-v" | "--on-variable" | "-j" | "--on-job-exit" => {
                    i += 1;
                    if i < options.len() {
                        i += 1;
                    }
                    continue;
                }
                _ => {
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }

    (named_args, description)
}

pub fn extract_word_string(w: &Word) -> Option<String> {
    let mut s = String::new();
    for p in &w.parts {
        match p {
            WordPart::Literal(lit) => s.push_str(lit),
            WordPart::SingleQuoted(sq) => s.push_str(sq),
            WordPart::DoubleQuoted(parts) => {
                for dp in parts {
                    if let WordPart::Literal(lit) = dp {
                        s.push_str(lit);
                    }
                }
            }
            _ => return None,
        }
    }
    Some(s)
}
