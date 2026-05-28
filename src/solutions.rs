use bitvec::{boxed::BitBox, prelude::bitbox};

use crate::{on_error, EXIT_CODE_OTHER_ERROR};
pub struct Candidate {
    literals: BitBox,
    framework_assumptions_count: usize
}

//Represents a candidate solution which might yield a complete soultion later on
impl Candidate {

    #[inline(always)]
    pub fn new(framework_assumptions_count: usize, literals_count: usize) -> Self {
        Self {
            literals: bitbox![0; literals_count],
            framework_assumptions_count
        }
    }

    #[inline(always)]
    pub fn contains_assumption(&self, assumption: usize) -> bool {
        debug_assert!(assumption < self.framework_assumptions_count);
        self.literals[assumption]
    }

    pub fn set(&mut self, assumption: usize, value: bool) -> bool {
        let Some(mut literal) = self.literals.get_mut(assumption) else {
            on_error("Failed to get literal", EXIT_CODE_OTHER_ERROR);
        };
        literal.replace(value)
    }

    #[inline(always)]
    pub fn get_assumption(&self) -> Vec<usize> {
        self.literals[0..self.framework_assumptions_count]
            .iter()
            .enumerate()
            .filter_map(|(index, value)| if *value { Some(index) } else { None })
            .collect()
    }
    
}

//Represents a found solution as a set of assumptions
pub struct Solution {
    assumptions: Vec<usize>
}

impl Solution {

    #[inline(always)]
    pub fn get_assumption(&self, index: usize) -> usize {
        self.assumptions[index]
    }

    #[inline(always)]
    pub fn assumptions(&self) -> std::slice::Iter<'_, usize> {
        self.assumptions.iter()
    }

    #[inline(always)]
    pub fn len_assumptions(&self) -> usize {
        self.assumptions.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.assumptions.is_empty()
    }
}

impl From<&Candidate> for Solution {

    #[inline(always)]
    fn from(candidate: &Candidate) -> Self {
        Solution {
            assumptions: candidate.get_assumption()
        }
    }
}

//Represents the found solutions and the current candidate solution.
//The candidate solution might be discarded if it is subsumed by an already found solution.
//An already found solution might be discarded if it is subsumed by a new candidate solution.
pub struct Solutions {
    candidate: Candidate,
    solutions: Vec<Solution>,
    watches: Vec<usize>,
    candidate_is_contained: bool
}

pub enum AssumptionAddResult {
    Ok,
    Subsumed,
    AlreadyExists
}

impl Solutions {

    #[inline(always)]
    pub fn new(framework_assumptions_count: usize, literals_count: usize) -> Solutions {
        Solutions {
            candidate: Candidate::new(framework_assumptions_count, literals_count),
            solutions: Vec::new(),
            watches: Vec::new(),
            candidate_is_contained: false
        }
    }    

    #[inline(always)]
    pub fn solutions(&self) -> std::slice::Iter<'_, Solution> {
        self.solutions.iter()
    }  

    #[inline(always)]
    pub fn commit(&mut self) {
        
        debug_assert!(!self.candidate_is_contained);
        let new_solution = Solution::from(&self.candidate);
        self.remove_superset_solutions(&new_solution);
        self.solutions.push(new_solution);
        self.watches.push(0);
        self.candidate_is_contained = true;
    }

    #[inline(always)]
    fn remove_superset_solutions(&mut self, new_solution: &Solution) {
        let mut index = 0;
        while index < self.solutions.len() {
            if Self::is_contained_in(new_solution, &self.solutions[index]) {
                self.solutions.swap_remove(index);
                self.watches.swap_remove(index);
            } else {
                index += 1;
            }
        }
    }

    //Test if x is contained in y
    #[inline(always)]
    fn is_contained_in(x: &Solution, y: &Solution) -> bool {
        let mut other_iter = y.assumptions();
        'outer: for assumption in x.assumptions() {
            while let Some(other_assumption) = other_iter.next() {
                if *assumption < *other_assumption {
                    return false;
                }

                if *assumption == *other_assumption {
                    continue 'outer;
                }
            }
            return false;
        }
        true
    }

    #[inline(always)]
    pub fn add_assumption(&mut self, assumption: usize) -> AssumptionAddResult {
        if self.candidate.set(assumption, true) {
            return AssumptionAddResult::AlreadyExists;
        }

        if self.is_subsumed(assumption) {
            let result = self.candidate.set(assumption, false);
            debug_assert!(result);
            return AssumptionAddResult::Subsumed;
        }

        AssumptionAddResult::Ok
    }

    #[inline(always)]
    pub fn is_candidate_contained(&self) -> bool {
        self.candidate_is_contained
    }

    #[inline(always)]
    pub fn add_literal(&mut self, literal: usize) -> bool {
        !self.candidate.set(literal, true)
    }

    #[inline(always)]
    pub fn remove_literal(&mut self, literal: usize) {
        let result = self.candidate.set(literal, false);
        debug_assert!(result)
    }   
    
    #[inline(always)]
    pub fn remove_assumption(&mut self, assumption: usize) {
        let result = self.candidate.set(assumption, false);
        debug_assert!(result);
        if self.candidate_is_contained {
            self.update_last_added_solution_watch(assumption);
            self.candidate_is_contained = false;
        }
    }

    #[inline(always)]
    fn update_last_added_solution_watch(&mut self, assumption: usize) {
        let last_index = self.solutions.len() - 1;
        let solution = &self.solutions[last_index];
        let watch = &mut self.watches[last_index];
        for (index, solution_assumption) in solution.assumptions().enumerate() {
            if *solution_assumption == assumption {
                *watch = index;
                return;
            }
        }
        on_error("Failed to update last added solution watch", EXIT_CODE_OTHER_ERROR);
    }

    #[inline(always)]
    fn is_subsumed(&mut self, added_assumption: usize) -> bool {
        for (solution, watch) in self.solutions.iter().zip(self.watches.iter_mut()) {
            if !Self::is_subsumed_for_solution_index(solution, &self.candidate, watch, added_assumption) {
                return true;
            }
        }
        false
    }

    #[inline(always)]
    fn is_subsumed_for_solution_index(solution: &Solution, candidate: &Candidate, watch: &mut usize, added_assumption: usize) -> bool {
        let initial_watch = *watch;

        if solution.get_assumption(initial_watch) != added_assumption {
            return true;
        }

        *watch += 1;
        
        loop {
            if *watch == solution.len_assumptions() {
                *watch = 0;
                continue;
            }

            if *watch == initial_watch {
                return false;
            }

            let assumption = solution.get_assumption(*watch);
            if !candidate.contains_assumption(assumption) {
                return true;
            }

            *watch += 1;
        }
    }
}