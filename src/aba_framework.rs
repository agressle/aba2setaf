//Represents an abaframework. The framework is already parsed and thus immuable as it will not be changed from this point on.

use std::ops::Range;
use crate::aba_rule::AbaRule;

#[derive(Clone, Debug)]
pub struct AbaFramework {
    rules: Box<[AbaRule]>,
    contraries: Box<[usize]>,
    head_rules: Box<[Option<Box<[usize]>>]>,
    literal_names: Box<[usize]>
}

impl AbaFramework {

    #[inline(always)]
    pub fn new(
        rules: Box<[AbaRule]>,
        contraries: Box<[usize]>,
        head_rules: Box<[Option<Box<[usize]>>]>,
        literal_names: Box<[usize]>
    ) -> Self {
        debug_assert!(contraries.len() <= literal_names.len());
        Self {
            rules,
            contraries,
            head_rules,
            literal_names
        }
    }

    #[inline(always)]
    pub fn get_count(&self) -> (usize, usize) {
        (self.contraries.len(), self.literal_names.len())
    }

    #[inline(always)]
    pub fn assumptions(&self) -> Range<usize> {
        0 .. self.contraries.len()
    }

    #[inline(always)]
    pub fn get_contrary(&self, literal: usize) -> usize {
        self.contraries[literal]
    }

    #[inline(always)]
    pub fn get_head_rules_for_literal(&self, literal: usize) -> &Option<Box<[usize]>> {
        &self.head_rules[literal]
    }

    #[inline(always)]
    pub fn get_rule(&self, index: usize) -> &AbaRule {
        &self.rules[index]
    }

    #[inline(always)]
    pub fn get_name_for_literal(&self, literal: usize) -> usize {
        self.literal_names[literal]
    }

    #[inline(always)]
    pub fn literal_is_assumption(&self, literal: usize) -> bool {
        literal < self.contraries.len()
    }

    #[cfg(debug_assertions)]
    pub fn debug_print(&self) {
        use debug_print::{debug_print, debug_println};
        debug_println!("Literals:");
        for literal in 0..self.literal_names.len() {
            if self.literal_is_assumption(literal) {
                debug_println!("    Assumption: {}, Contrary: {}", self.get_name_for_literal(literal), self.get_name_for_literal(self.get_contrary(literal)));
            }
        }
        for literal in 0..self.literal_names.len() {
            if !self.literal_is_assumption(literal) {
                debug_println!("    Literal: {}", self.get_name_for_literal(literal));
            }
        }
        debug_println!("Rules:");
        for (index, rule) in self.rules.iter().enumerate() {
            debug_print!("    Rule {index}: {} <- ", self.get_name_for_literal(rule.get_head()));
            for assumption in rule.get_body_assumptions() {
                debug_print!("{} ", self.get_name_for_literal(*assumption));
            }
            debug_print!("| ");
            for literal in rule.get_body_literals() {
                debug_print!("{} ", self.get_name_for_literal(*literal));
            }
            debug_println!();
        }

        debug_println!("Head rules:");
        for (literal, head_rule) in self.head_rules.iter().enumerate() {
            debug_print!("    Literal {}:", self.get_name_for_literal(literal));
            if let Some(head_rule) = head_rule {
                for rule in head_rule.iter() {
                    debug_print!("{} ", rule);
                }
            }
            debug_println!();
        }
    }
}