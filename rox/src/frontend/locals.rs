use crate::{RoxString, Token, TokenType, DEBUG_MODE};

use super::LOCALS_COUNT;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Local {
    pub name: Option<Token>,
    pub depth: Option<usize>,
}

impl Local {
    pub fn new(name: &Token, depth: usize) -> Local {
        Local {
            name: Some(name.clone()),
            depth: Some(depth),
        }
    }
}

pub struct Locals {
    locals: [Local; LOCALS_COUNT],
    count: usize,
}

impl Locals {
    pub fn new() -> Locals {
        let locals = [(); LOCALS_COUNT].map(|_| Local::default());
        Locals { locals, count: 0 }
    }

    pub fn size(&self) -> usize {
        self.count
    }

    pub fn initialize_variable(&mut self, scope_depth: usize) {
        self.locals[self.count - 1].depth = Some(scope_depth);
    }

    pub fn add_local(&mut self, token: &Token, depth: usize) {
        self.locals[self.count] = Local::new(token, depth);
        self.count += 1;

        if DEBUG_MODE {
            println!("Added local variable at index {}", self.count - 1);
        }
    }

    pub fn remove_locals(&mut self, scope_depth: usize) -> usize {
        let mut num_locals_removed = 0;

        for idx in (0..self.count).rev() {
            let local = &self.locals[idx];
            if let Some(depth) = local.depth {
                if depth > scope_depth {
                    num_locals_removed += 1;
                    self.count -= 1;
                }
            }
        }

        num_locals_removed
    }

    pub fn local_is_doubly_declared(&self, looking_for: &Token, scope_depth: usize) -> bool {
        for idx in (0..self.count).rev() {
            let local = &self.locals[idx];
            if let Some(depth) = local.depth {
                if depth < scope_depth {
                    return false;
                }
            }

            if let Some(name) = &local.name {
                let local_str = match &name.token_type {
                    TokenType::Identifier(s) => s,
                    _ => panic!("Local string not an identifier"),
                };

                let looking_for_str = match &looking_for.token_type {
                    TokenType::Identifier(s) => s,
                    _ => panic!("Looking for string not an identifier!"),
                };

                if DEBUG_MODE {
                    println!("Comparing {local_str} to {looking_for_str}.");
                }

                if *local_str == *looking_for_str {
                    return true;
                }
            }
        }

        false
    }

    pub fn resolve_local(&self, local_id: &RoxString) -> (bool, Option<usize>) {
        for idx in (0..self.count).rev() {
            let local = &self.locals[idx];
            if let Some(token) = &local.name {
                if let TokenType::Identifier(string) = &token.token_type {
                    if **string == *local_id {
                        if DEBUG_MODE {
                            println!("Resolving local variable {}", local_id);
                        }
                        if local.depth.is_none() {
                            return (false, None);
                        }

                        return (true, Some(idx));
                    }
                }
            }
        }
        (true, None)
    }
}
