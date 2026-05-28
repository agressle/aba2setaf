//Used to build the aba framework
//Verifies that the encoding is valid, remove subsumed rules and computes which literals are reachable from assumptions

use core::fmt;
use std::{collections::HashMap, ffi::OsStr};
use bitvec::{bitbox, boxed::BitBox};
use tokio::{fs::File, io::{self, AsyncBufReadExt, BufReader}};
use crate::{aba_framework::AbaFramework, aba_rule::AbaRule, on_error, trie::Trie, EXIT_CODE_OTHER_ERROR};

#[derive(Clone, Debug)]
struct AbaLiteral {
    index: usize,
    is_assumption: bool,
    contrary: Option<usize>
}

impl AbaLiteral {

    #[inline(always)]
    pub fn new(index: usize) -> AbaLiteral {
        AbaLiteral {
            index,
            is_assumption: false,
            contrary: None
        }
    }

    #[inline(always)]
    pub fn mark_as_assumption(&mut self) {
        self.is_assumption = true;
    }

    #[inline(always)]
    pub fn is_assumption(&self) -> bool {
        self.is_assumption
    }

    #[inline(always)]
    pub fn set_contrary(&mut self, contrary: usize) -> bool {
        match self.contrary{
            None => {self.contrary = Some(contrary); true},
            Some(other) => contrary == other
        }
    }

    #[inline(always)]
    pub fn get_index(&self) -> usize {
        self.index
    }

    #[inline(always)]
    pub fn get_contrary(&self) -> Option<usize> {
        self.contrary
    }
}


#[derive(Clone, Debug)]
pub enum AbaFrameworkEncodingError {
    UnexpectedLine(String),
    HeaderLineMissing,
    HeaderLineInvalid(String),
    AssumptionLineInvalid(String),
    ContraryLineInvalid(String),
    RuleLineInvalid(String),
    MissingContraries(usize),
    FrameworkNotFlat(usize),
}

impl fmt::Display for AbaFrameworkEncodingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedLine(line) => write!(f, "found unexpected line: {line}"),
            Self::HeaderLineMissing => write!(f, "found no header line"),
            Self::HeaderLineInvalid(line) => write!(f, "found invalid header line: {line}"),
            Self::AssumptionLineInvalid(line) => write!(f, "found invalid assumption line: {line}"),
            Self::ContraryLineInvalid(line) => write!(f, "found invalid contrary line: {line}"),
            Self::RuleLineInvalid(line) => write!(f, "found invalid rule line: {line}"),
            Self::MissingContraries(index) => write!(f, "assumption {index} does not have a contrary"),
            Self::FrameworkNotFlat(index) => write!(f, "rule {index} is not flat")
        }
    }
}

#[derive(Debug)]
pub enum AbaFrameworkParsingError {
    IoError(io::Error),
    FrameworkError(AbaFrameworkEncodingError)
}

impl fmt::Display for AbaFrameworkParsingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(io_error) => write!(f, "failed to parse aba framework due to io error: {io_error}"),
            Self::FrameworkError(framework_error) => write!(f, "failed to parse aba framework due to an encoding error: {framework_error}")
        }
    }
}

impl From<io::Error> for AbaFrameworkParsingError {
    #[inline(always)]
    fn from(value: io::Error) -> Self {
        Self::IoError(value)
    }
}

impl From<AbaFrameworkEncodingError> for AbaFrameworkParsingError {
    #[inline(always)]
    fn from(value: AbaFrameworkEncodingError) -> Self {
        Self::FrameworkError(value)
    }
}

#[derive(Clone, Debug)]
pub struct AbaFrameworkBuilder {
    literals: Box<[AbaLiteral]>,
    rules: Vec<AbaRule>,
    head_rules: HashMap<usize, Vec<usize>>, //A mapping from literal index to a vec of rules indices in which the literal is the head,
    assumption_count: usize,
    reachable_literals: Option<BitBox>
}

impl AbaFrameworkBuilder {
    fn parse_literal_number(literal_number: &str) -> Option<usize> {
        let Ok(literal_number) = literal_number.parse::<usize>() else {
            return None;
        };
        if literal_number == 0 {
            return None;
        }
        Some(literal_number)
    }

    #[inline(always)]
    fn mark_as_assumption(&mut self, literal_number: &str) -> bool {
        let Some(literal_number) = Self::parse_literal_number(literal_number) else {
            return false;
        };
        
        let Some(literal) = self.literals.get_mut(literal_number) else {
            return false;
        };
        
        if !literal.is_assumption() {
            literal.mark_as_assumption();
            self.assumption_count += 1;
        }

        true
    }

    #[inline(always)]
    fn set_contrary(&mut self, literal_number: &str, contrary_number: &str) -> bool {
        let Some(literal_number) = Self::parse_literal_number(literal_number) else {
            return false;
        };

        let Some(contrary_number) = Self::parse_literal_number(contrary_number) else {
            return false;
        };

        if contrary_number >= self.literals.len() {
            return false;
        }

        let Some(literal) = self.literals.get_mut(literal_number) else {
            return false;
        };

        literal.set_contrary(contrary_number)
    }

    #[inline(always)]
    fn add_rule(&mut self, literals: &[&str]) -> bool {
        let mut parsed_literals = Vec::with_capacity(literals.len());
        for literal in literals {
            let Some(literal_number) = Self::parse_literal_number(literal) else {
                return false;
            };
            parsed_literals.push(literal_number);
        }

        let parsed_literals = parsed_literals.into_boxed_slice();
        if parsed_literals.iter().any(|index| *index >= self.literals.len()) {
            return false;
        }

        let Some(head) = parsed_literals.first() else {
            return false;
        };

        let mut body_assumptions = Vec::new();
        let mut body_literals = Vec::new();

        for literal in parsed_literals[1..].iter() {
            if self.literals[*literal].is_assumption() {
                body_assumptions.push(*literal);
            } else {
                body_literals.push(*literal);
            }
        }

        let rule = AbaRule::new(*head, body_assumptions, body_literals);

        self.head_rules.entry(rule.get_head())
            .or_insert(Vec::new())
            .push(self.rules.len());

        self.rules.push(rule);
        true
    }

    #[inline(always)]
    fn literal_iterator(&self) -> std::slice::Iter<'_, AbaLiteral> {
        self.literals[1..].iter()
    }    

    #[inline(always)]
    pub async fn parse(path: &OsStr) -> Result<Box<Self>, AbaFrameworkParsingError> {
        let file = File::open(path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        //Skip comments at the start
        let mut header_line = None;
        while let Some(line) = lines.next_line().await? {
            let trimmed_line = line.trim();
            if trimmed_line.starts_with("#") {
                continue;
            }
            if trimmed_line.starts_with("p") {
                header_line = Some(line);
                break;
            }
            return Err(AbaFrameworkEncodingError::UnexpectedLine(line).into())
        }
        
        //Parse header line
        let Some(line) = header_line else {
            return Err(AbaFrameworkEncodingError::HeaderLineMissing.into());
        };        
        let Ok(["p", "aba", header_size]) = <[&str; 3]>::try_from(line.trim().split_ascii_whitespace().collect::<Vec<_>>()) else {
            return Err(AbaFrameworkEncodingError::HeaderLineInvalid(line).into());
        };        
        let Ok(number_of_literals) = header_size.parse::<usize>() else {
            return Err(AbaFrameworkEncodingError::HeaderLineInvalid(line).into());
        };

        //Add literal 0 to avoid constantly adding offset
        let mut framework = Box::new(Self {
            literals: (0..=number_of_literals).map(|index| AbaLiteral::new(index)).collect(),
            rules: Vec::new(),
            head_rules: HashMap::new(),
            assumption_count: 0,
            reachable_literals: None
        });

        //Parse remaining lines
        while let Some(line) = lines.next_line().await? {
            let trimmed_line = line.trim();
            if trimmed_line.starts_with("#") {
                continue;
            }
            
            if trimmed_line.starts_with("a") {
                let Ok(["a", literal_number]) = <[&str; 2]>::try_from(line.trim().split_ascii_whitespace().collect::<Vec<_>>()) else {
                    return Err(AbaFrameworkEncodingError::AssumptionLineInvalid(line).into());
                };
                if !framework.mark_as_assumption(literal_number) {
                    return Err(AbaFrameworkEncodingError::AssumptionLineInvalid(line).into());
                }
                continue;
            }

            if trimmed_line.starts_with("c") {
                let Ok(["c", literal_number, contrary_number]) = <[&str; 3]>::try_from(line.trim().split_ascii_whitespace().collect::<Vec<_>>()) else {
                    return Err(AbaFrameworkEncodingError::ContraryLineInvalid(line).into());
                };
                if !framework.set_contrary(literal_number, contrary_number) {
                    return Err(AbaFrameworkEncodingError::ContraryLineInvalid(line).into());
                }
                continue;
            }

            if trimmed_line.starts_with("r") {
                let split : Vec<&str> = trimmed_line.split_ascii_whitespace().collect();
                let Some(&"r") = split.first() else {
                    return Err(AbaFrameworkEncodingError::RuleLineInvalid(line).into());
                };
                if !framework.add_rule(&split.as_slice()[1..]) {
                    return Err(AbaFrameworkEncodingError::RuleLineInvalid(line).into());
                }
                continue;
            }
            return Err(AbaFrameworkEncodingError::UnexpectedLine(line).into())
        }

        //Check that all assumptions have contraries
        for (index, literal) in framework.literal_iterator().enumerate() {
            if literal.is_assumption() && literal.get_contrary().is_none() {
                return Err(AbaFrameworkEncodingError::MissingContraries(index + 1).into());
            }
        }

        //Check that all rules are flat
        for (index, rule) in framework.rules.iter().enumerate() {
            if  framework.literals[rule.get_head()].is_assumption() {
                return Err(AbaFrameworkEncodingError::FrameworkNotFlat(index + 1).into());
            }
        }

        Ok(framework)
    }

    //Computes which literals are reachable from assumptions. If not, they cannot be used in any tree decompisiton
    pub fn compute_reachable_literals(&mut self) {
        let mut reachable_literals = bitbox![0; self.literals.len()];
        let mut literals_to_add = Vec::new();

        //Assumptions are trivially reachable
        for literal in self.literal_iterator() {
            if literal.is_assumption() {
                reachable_literals.set(literal.get_index(), true);
            }
        }

        let mut watches = vec![Vec::new(); self.literals.len()];
        let mut watch_index = vec![0; self.rules.len()];
        
        //Initialize watches to the first literal of every rule
        for (rule_index, rule) in self.rules.iter().enumerate() {
            if let Some(first_body_literal) = rule.get_body_literals().first() {
                watches[*first_body_literal].push(rule_index);
            } else {
                literals_to_add.push(rule.get_head()); //Empty rule body -> literal is reachable
            }
        }

        //Propagate
        while let Some(literal) = literals_to_add.pop() {
            let Some(mut status) = reachable_literals.get_mut(literal) else {
                on_error("Failed to get literal", EXIT_CODE_OTHER_ERROR);
            };

            if status.replace(true) {
                continue;
            }
            drop(status);

            let rules_to_check: Vec<_> = watches[literal].drain(..).collect();
            for rule_index in rules_to_check {
                let rule = &self.rules[rule_index];
                let current_watch = &mut watch_index[rule_index];
                let mut found = false;
                for literal in rule.get_body_literals().iter().skip(*current_watch + 1) {
                    *current_watch += 1;
                    if !reachable_literals[*literal] {
                        watches[*literal].push(rule_index);
                        found = true;
                        break;
                    }
                }

                if !found {
                    literals_to_add.push(rule.get_head());
                }
            }
        }

        self.reachable_literals = Some(reachable_literals);
    }

    #[inline(always)]
    //We remove all rules that are subsume or contain unreachable literals. Then, we sort the head rules for every literal by the number of body literals to aid in search
    pub fn update_and_sort_headrule(&mut self) {
        let Some(reachable_literals) = &self.reachable_literals else {
            on_error("Tried to update and sort head rules without computing reachable literals", EXIT_CODE_OTHER_ERROR);
        };

        let mut rules_to_delete = bitbox![0; self.rules.len()];

        //Mark all rules to delete that contain unreachable literals and clear those to safe memory
        for (rule_index, rule) in self.rules.iter_mut().enumerate() {
            if rule.get_body_literals().iter().any(|literal| !reachable_literals[*literal]) {
                rules_to_delete.set(rule_index, true);
                rule.clear_body();
            }
        }

        //Remove all rules that are marked for deletion from the head rules, check for subsumption and compute new list of sorted head rules
        for (_, rules) in self.head_rules.iter_mut() {
            let (new_rules, new_rules_to_delete) = Self::update_head_rules(&self.rules, rules, &rules_to_delete);
            *rules = new_rules;
            for rule in new_rules_to_delete {
                let Some(status) = rules_to_delete.get_mut(rule) else {
                    on_error("Failed to index into rules to delete", EXIT_CODE_OTHER_ERROR);
                };
                if !status.clone() {
                    status.commit(true);
                    self.rules[rule].clear_body();
                }
            }
        }
    }

    #[inline(always)]
    fn update_head_rules(all_rules: &Vec<AbaRule>, rules: &Vec<usize>, rules_to_delete: &BitBox) -> (Vec<usize>, Vec<usize>) {
       //The index of the rule, wether or not the rule is subsumed and the body assumptions and literals sorted
       let mut relevant_rules = Vec::new();
       for rule_index in rules.iter() {
            if rules_to_delete[*rule_index] {
                continue;
            }
            let mut body_literals = Vec::new();
            let rule = &all_rules[*rule_index];
            body_literals.extend_from_slice(rule.get_body_assumptions());
            body_literals.extend_from_slice(rule.get_body_literals());
            body_literals.sort_unstable();
            relevant_rules.push((rule_index, false, body_literals));
        }
        relevant_rules.sort_unstable_by(|a, b|a.2.len().cmp(&b.2.len()));

        //Check for subsumption
        let mut trie = Trie::new();
        for (_, subsumed, body_literals) in relevant_rules.iter_mut() {
            if trie.contains_subset_of(body_literals) {
                *subsumed = true;
            } else {
                trie.insert(body_literals);
            }
        }

        let new_rules_to_delete = relevant_rules.iter()
            .filter_map(|(index, subsumed, _)| if *subsumed { Some(**index) } else { None })
            .collect::<Vec<_>>();

        let new_head_rules = relevant_rules.into_iter()
            .filter_map(|(index, subsumed, _)| if !subsumed { Some(*index) } else { None })
            .collect::<Vec<_>>();

        (new_head_rules, new_rules_to_delete)
    }

    #[cfg(debug_assertions)]
    pub fn debug_print(&self) {
        use debug_print::{debug_print, debug_println};
        debug_println!("Literals:");
        for literal in self.literal_iterator() {
            if literal.is_assumption() {
                let Some(contrary) = literal.get_contrary() else {
                    on_error("Found assumption that does not have a contrary in debug print", EXIT_CODE_OTHER_ERROR);
                };
                debug_println!("    Assumption: {}, Contrary: {}", literal.get_index(), contrary);
            }
        }

        for literal in self.literal_iterator() {
            if !literal.is_assumption {
                debug_println!("    Literal: {}", literal.get_index());
            }
        }
        debug_println!("Rules:");
        for (index, rule) in self.rules.iter().enumerate() {
            debug_print!("    Rule {index}: {} <- ", rule.get_head());
            for assumption in rule.get_body_assumptions() {
                debug_print!("{} ", assumption);
            }
            debug_print!("| ");
            for literal in rule.get_body_literals() {
                debug_print!("{} ", literal);
            }
            debug_println!();
        }

        debug_println!("Head rules:");
        for (literal, head_rule) in self.head_rules.iter() {
            debug_print!("    Literal {}:", literal);
            for rule in head_rule.iter() {
                debug_print!("{} ", rule);
            }
            debug_println!();
        }
    }
}

//Create an abaframework and a bitbox of reachable literals from the builder
impl Into<(AbaFramework, BitBox)> for AbaFrameworkBuilder {
    #[inline(always)]
    fn into(mut self) -> (AbaFramework, BitBox) {
        let mapping_from_name_to_new_index = remap_literal_numbers(&mut self);
        let number_of_assumptions = self.assumption_count;
        let number_of_literals = self.literals.len() - 1; //-1 because of dummy literal 0

        let mut contraries = vec![0; number_of_assumptions];
        let mut literal_names = vec![0; number_of_literals];
        let mut head_rules = vec![None; number_of_literals];
        let rules = self.rules.into_boxed_slice();
        
        for literal in self.literals.into_iter().skip(1) { //Skip dummy literal 0
            let new_index = mapping_from_name_to_new_index[literal.get_index()];
            if new_index < number_of_assumptions {
                if !literal.is_assumption() {
                    on_error("Found literal that is not marked as assumption but has an invalid index", EXIT_CODE_OTHER_ERROR);
                }
                let Some(contrary) = literal.get_contrary() else {
                    on_error("Found assumption that does not have a contrary", EXIT_CODE_OTHER_ERROR);
                };
                contraries[new_index] = contrary;
            } else {
                if literal.is_assumption() {
                    on_error("Found literal that is marked as assumption but has an invalid index", EXIT_CODE_OTHER_ERROR);
                }
            }

            if let Some(head_rules_vec) = self.head_rules.remove(&literal.get_index()) {
                head_rules[new_index] = Some(head_rules_vec.into_boxed_slice());
            } else {
                head_rules[new_index] = None;
            }

            literal_names[new_index] = literal.get_index();
        }

        let Some(reachable_literals) = &self.reachable_literals else {
            on_error("Tried to create abaframework without computing reachable literals", EXIT_CODE_OTHER_ERROR);
        };

        let reachable_literal = literal_names.iter()
            .map(|index| reachable_literals[*index])
            .collect();

        (AbaFramework::new(
            rules,
            contraries.into_boxed_slice(),
            head_rules.into_boxed_slice(),
            literal_names.into_boxed_slice()
        ), reachable_literal)

    }
}

#[inline(always)]
fn remap_literal_numbers(builder: &mut AbaFrameworkBuilder) -> Vec<usize> {
    let mut mapping_from_name_to_new_index = vec![0; builder.literals.len()];
    let mut next_assumption_index = 0;
    let mut next_literal_index = builder.assumption_count;
    for literal in builder.literals.iter().skip(1) { //Skip dummy literal 0
        if literal.is_assumption() {
            mapping_from_name_to_new_index[literal.get_index()] = next_assumption_index;
            next_assumption_index += 1;
        } else {
            mapping_from_name_to_new_index[literal.get_index()] = next_literal_index;
            next_literal_index += 1;
        }
    }

    for literal in builder.literals.iter_mut().skip(1) { //Skip dummy literal 0
        let contary = &mut literal.contrary;
        if let Some(contary_index) = contary {
            *contary = Some(mapping_from_name_to_new_index[*contary_index]);
        }
    }

    for rule in builder.rules.iter_mut() {
        rule.apply_mapping(&mapping_from_name_to_new_index);
    }

    mapping_from_name_to_new_index
}