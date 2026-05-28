use std::{collections::{HashMap, HashSet}, ffi::OsStr, io::SeekFrom, sync::OnceLock };
use bitvec::boxed::BitBox;
use tokio::{fs::{self, File, OpenOptions}, io::{AsyncSeekExt, AsyncWriteExt, BufWriter}, sync::mpsc};
use debug_print::{debug_print, debug_println};
use crate::{aba_framework::AbaFramework, aba_framework_builder::AbaFrameworkBuilder, on_error, solutions::{self, Solutions}, tree::Tree, EXIT_CODE_FILE_EXISTS, EXIT_CODE_IO_ERROR, EXIT_CODE_OK, EXIT_CODE_OTHER_ERROR};

//Translate the ABA framework to an SETAF
pub async fn translate(mut builder: Box<AbaFrameworkBuilder>, destination: &OsStr, overwrite: bool, asp: bool) -> i32 {
    if !overwrite {
        if fs::metadata(destination).await.is_ok() {
            eprintln!("Destination file already exists");
            return EXIT_CODE_FILE_EXISTS;
        }
    }

    let mut file = OpenOptions::new();
    let file = file.write(true);
    let file = if overwrite {
        file.create(true).truncate(true)
    } else {
        file.create_new(true)
    };
    let file = match file.open(destination).await {
        Result::Ok(file) => file,
        Result::Err(e) => {
            eprintln!("Failed to open file for writing: {e}");
            return EXIT_CODE_IO_ERROR;
        }
    };

    builder.compute_reachable_literals();
    builder.update_and_sort_headrule();
    #[cfg(debug_assertions)] builder.debug_print();
    let (framework, reachable_literals): (AbaFramework, BitBox) = (*builder).into();
    #[cfg(debug_assertions)] framework.debug_print();

    let (tx, rx) = mpsc::channel(1);
    let mut mapping = HashMap::new();
    let mut next_index: usize = 1;

    static FRAMEWORK : OnceLock<AbaFramework> = OnceLock::new();
    FRAMEWORK.get_or_init(|| framework);
    let Some(framework) = FRAMEWORK.get() else {
        on_error("Failed to get framework", EXIT_CODE_OTHER_ERROR);
    };

    static REACHABLE_LITERALS : OnceLock<BitBox> = OnceLock::new();
    REACHABLE_LITERALS.get_or_init(|| reachable_literals);
    let Some(reachable_literals) = REACHABLE_LITERALS.get() else {
        on_error("Failed to get reachable literals", EXIT_CODE_OTHER_ERROR);
    };

    #[cfg(debug_assertions)]
    for (index, literal) in reachable_literals.iter().enumerate() {
        debug_println!("Literal {} reachable: {literal}", framework.get_name_for_literal(index));
    }

    //Start the search for every assumption in parallel
    //To report back the results we use a channel
    for literal in framework.assumptions() {
        mapping.insert(literal, next_index.to_string().into_bytes().into_boxed_slice());
        next_index += 1;
        tokio::spawn(start_for_assumption(framework, literal, reachable_literals, tx.clone()));
    }
    drop(tx);

    //Write the found solutions to disk
    let writer_return_value =
        if asp {
            writer_asp(file, rx, mapping).await
        } else {
            writer(file, destination, rx, next_index - 1, mapping).await
        };

    if let Err(writer_return_value) = writer_return_value {
        return writer_return_value
    };

    EXIT_CODE_OK
}

//Write in DIMACS-like format
async fn writer(file: File, destination: &OsStr, mut rx: mpsc::Receiver<(usize, Solutions)>, assumption_count: usize, mapping: HashMap<usize, Box<[u8]>>) -> Result<(), i32> {
    let mut attack_count : usize = 0;
    let mut writer = BufWriter::new(file);

    let heading_start = format!("{assumption_count}");
    let placeholder = format!("{}", usize::MAX); //Reverse space at the top for the number of attacks as we do not know them yet. We will fill them in later when all attacks have been written.

    //Write preamble
    if let Err(e) = writer.write_all(format!("{} {} 0\n", heading_start, placeholder).as_bytes()).await {
        eprintln!("Failed to write heading: {e}.");
        return Err(EXIT_CODE_IO_ERROR);
    }

    //Write the attacks
    while let Some((index, solutions)) = rx.recv().await {
        let Some(head_bytes) = mapping.get(&index) else {
            eprintln!("Failed to get mapped index for rule head.");
            return Err(EXIT_CODE_OTHER_ERROR);
        };
        
        for solution in solutions.solutions() {
            if let Err(e) = writer.write_all(head_bytes).await {
                eprintln!("Failed to write rule head: {e}.");
                return Err(EXIT_CODE_IO_ERROR);
            }

            if solution.is_empty() {
                if let Err(e) = writer.write_all(b" ").await {
                    eprintln!("Failed to write body delimiter: {e}.");
                    return Err(EXIT_CODE_IO_ERROR);
                }
                if let Err(e) = writer.write_all(head_bytes).await {
                    eprintln!("Failed to write rule head: {e}.");
                    return Err(EXIT_CODE_IO_ERROR);
                }
            } else {
                for assumption in solution.assumptions() {
                    let Some(body_bytes) = mapping.get(assumption) else {
                        eprintln!("Failed to get mapped index for rule body.");
                        return Err(EXIT_CODE_OTHER_ERROR);
                    };  

                    if let Err(e) = writer.write_all(b" ").await {
                        eprintln!("Failed to write body delimiter: {e}.");
                        return Err(EXIT_CODE_IO_ERROR);
                    }

                    if let Err(e) = writer.write_all(body_bytes).await {
                        eprintln!("Failed to write body assumption: {e}.");
                        return Err(EXIT_CODE_IO_ERROR);
                    }
                }
            }

            if let Err(e) = writer.write_all(b" 0\n").await {
                eprintln!("Failed to write rule ending: {e}.");
                return Err(EXIT_CODE_IO_ERROR);
            }
            attack_count += 1;
        }
    }
    
    if let Err(e) = writer.flush().await {
        eprintln!("Failed to flush writer: {e}.");
        return Err(EXIT_CODE_IO_ERROR);
    }

    drop(writer);
    
    //Update the number of attacks in the preamble
    let mut file = match OpenOptions::new().write(true).open(destination).await {
        Ok(file) => file,
        Err(e) => {
            eprintln!("Failed to reopen file: {e}.");
            return Err(EXIT_CODE_IO_ERROR);
        }
    };

    let mut attack_count_string = attack_count.to_string();
    let length_diff = placeholder.len() - attack_count_string.len();
    let padding = " ".repeat(length_diff);
    attack_count_string.push_str(" 0");
    attack_count_string.push_str(&padding);
    attack_count_string.push_str("\n");
    
    if let Err(e) = file.seek(SeekFrom::Start(heading_start.len() as u64 + 1)).await {
        eprintln!("Failed to seek: {e}.");
        return Err(EXIT_CODE_IO_ERROR);
    }
    if let Err(e) = file.write(attack_count_string.as_bytes()).await{
        eprintln!("Failed to write attack count: {e}.");
        return Err(EXIT_CODE_IO_ERROR);
    }
    if let Err(e) = file.flush().await{
        eprintln!("Failed to flush file: {e}.");
        return Err(EXIT_CODE_IO_ERROR);
    }    

    Ok(())
}

//Write using the asp format
async fn writer_asp(file: File, mut rx: mpsc::Receiver<(usize, Solutions)>, mapping: HashMap<usize, Box<[u8]>>) -> Result<(), i32> {
    let mut attack_count : usize = 1;
    let mut writer = BufWriter::new(file);

    let ending_bytes: Box<[u8]> = ").\n".bytes().collect();
    let delimiter: Box<[u8]> = ",".bytes().collect();
    let argument_start_bytes: Box<[u8]> = "arg(".bytes().collect();
    let attack_start_bytes: Box<[u8]> = "att(".bytes().collect();
    let member_start_bytes: Box<[u8]> = "mem(".bytes().collect();

    //Write the assumptions atoms
    for value in mapping.values() {
        if let Err(e) = writer.write_all(&argument_start_bytes).await {
            eprintln!("Failed to write argument start: {e}.");
            return Err(EXIT_CODE_IO_ERROR);
        }

        if let Err(e) = writer.write_all(value).await {
            eprintln!("Failed to write argument: {e}.");
            return Err(EXIT_CODE_IO_ERROR);
        }

        if let Err(e) = writer.write_all(&ending_bytes).await {
            eprintln!("Failed to write argument end: {e}.");
            return Err(EXIT_CODE_IO_ERROR);
        }
    }

    //Write the attacks
    while let Some((index, solutions)) = rx.recv().await {
        let Some(head_bytes) = mapping.get(&index) else {
            eprintln!("Failed to get mapped index for rule head.");
            return Err(EXIT_CODE_OTHER_ERROR);
        };

        //Write the attack atom
        for solution in solutions.solutions() {
            if let Err(e) = writer.write_all(&attack_start_bytes).await {
                eprintln!("Failed to write attack start: {e}.");
                return Err(EXIT_CODE_IO_ERROR);
            }
            if let Err(e) = writer.write_all(attack_count.to_string().as_bytes()).await {
                eprintln!("Failed to write attack name: {e}.");
                return Err(EXIT_CODE_IO_ERROR);
            }
            if let Err(e) = writer.write_all(&delimiter).await {
                eprintln!("Failed to write attack delimiter: {e}.");
                return Err(EXIT_CODE_IO_ERROR);
            }
            if let Err(e) = writer.write_all(&head_bytes).await {
                eprintln!("Failed to write attack target: {e}.");
                return Err(EXIT_CODE_IO_ERROR);
            }
            if let Err(e) = writer.write_all(&ending_bytes).await {
                eprintln!("Failed to write attack end: {e}.");
                return Err(EXIT_CODE_IO_ERROR);
            }

            //Write the member atoms
            if solution.is_empty() {
                if let Err(e) = writer.write_all(&member_start_bytes).await {
                    eprintln!("Failed to write member start: {e}.");
                    return Err(EXIT_CODE_IO_ERROR);
                }
                if let Err(e) = writer.write_all(attack_count.to_string().as_bytes()).await {
                    eprintln!("Failed to write member name: {e}.");
                    return Err(EXIT_CODE_IO_ERROR);
                }
                if let Err(e) = writer.write_all(&delimiter).await {
                    eprintln!("Failed to write member delimiter: {e}.");
                    return Err(EXIT_CODE_IO_ERROR);
                }
                if let Err(e) = writer.write_all(&head_bytes).await {
                    eprintln!("Failed to write member target: {e}.");
                    return Err(EXIT_CODE_IO_ERROR);
                }
                if let Err(e) = writer.write_all(&ending_bytes).await {
                    eprintln!("Failed to write member end: {e}.");
                    return Err(EXIT_CODE_IO_ERROR);
                }
            } else {
                for assumption in solution.assumptions() {
                    let Some(body_bytes) = mapping.get(assumption) else {
                        eprintln!("Failed to get mapped index for rule body.");
                        return Err(EXIT_CODE_OTHER_ERROR);
                    };

                    if let Err(e) = writer.write_all(&member_start_bytes).await {
                        eprintln!("Failed to write member start: {e}.");
                        return Err(EXIT_CODE_IO_ERROR);
                    }
                    if let Err(e) = writer.write_all(attack_count.to_string().as_bytes()).await {
                        eprintln!("Failed to write member name: {e}.");
                        return Err(EXIT_CODE_IO_ERROR);
                    }
                    if let Err(e) = writer.write_all(&delimiter).await {
                        eprintln!("Failed to write member delimiter: {e}.");
                        return Err(EXIT_CODE_IO_ERROR);
                    }
                    if let Err(e) = writer.write_all(&body_bytes).await {
                        eprintln!("Failed to write member target: {e}.");
                        return Err(EXIT_CODE_IO_ERROR);
                    }
                    if let Err(e) = writer.write_all(&ending_bytes).await {
                        eprintln!("Failed to write member end: {e}.");
                        return Err(EXIT_CODE_IO_ERROR);
                    }
                }
            }

            attack_count += 1;
        }
    }
    
    if let Err(e) = writer.flush().await {
        eprintln!("Failed to flush writer: {e}.");
        return Err(EXIT_CODE_IO_ERROR);
    }
    Ok(())
}

//The entry for the search for each assumption
#[inline(always)]
async fn start_for_assumption(framework: &AbaFramework, assumption: usize, reachable_literals: &BitBox, tx: mpsc::Sender<(usize, Solutions)>) {
    let contrary = framework.get_contrary(assumption);

    //If the contrary is not reachable, there is no way to build a complete tree derivation for it
    if !reachable_literals[contrary] {
        return;
    }

    debug_println!("Processing assumption {} with contrary {}", framework.get_name_for_literal(assumption), framework.get_name_for_literal(contrary));
    let solutions = process_contrary(framework, contrary);
    if let Err(e) = tx.send((assumption, solutions)).await {
        on_error(&format!("Failed to send solution: {e}."), EXIT_CODE_OTHER_ERROR)
    }
}


enum RequiredElement<'a> {
    LiteralInitial { child_index: usize, literal: usize }, //A literal that has been expanded from the parent node but not yet visited
    LiteralSkipped { child_index: usize, literal: usize }, //A literal that has already been visited by already contained in the candiate solution and thus skipped
    LiteralAdded { child_index: usize, literal: usize, head_rules: &'a Box<[usize]>, next_rule_index: usize, children_count: usize, next_child_index: usize }, //A literal that has been expanded and added to the current search tree together with the next head rule to expand and the next child node to visit
    Assumption { child_index: usize, assumption: usize, first_pass: bool, was_added: bool } //An assumption literal and wether it has already been visited or not and if it was added to the candiate solution or if it was already contained and thus skipped
}

impl<'a> RequiredElement<'a> {
    pub const fn get_child_index(&self) -> usize {
        match self {
            RequiredElement::LiteralInitial { child_index, .. } => { *child_index },
            RequiredElement::LiteralSkipped { child_index, .. } => { *child_index },
            RequiredElement::LiteralAdded { child_index, .. } => { *child_index },
            RequiredElement::Assumption { child_index, .. } => { *child_index },
        }
    }

    #[cfg(debug_assertions)]
    pub const fn get_literal_index(&self) -> usize {
        match self {
            RequiredElement::LiteralInitial { literal, .. } => { *literal },
            RequiredElement::LiteralSkipped { literal, .. } => { *literal },
            RequiredElement::LiteralAdded { literal, .. } => { *literal },
            RequiredElement::Assumption { assumption, .. } => { *assumption },
        }
    }    
}

enum ProcessingPredecessorResult {
    Parent,
    Child(bool, usize)
}

struct ProcessingResult {
    next_node: Option<usize>,
    result: ProcessingPredecessorResult
}

impl ProcessingResult {
    pub fn new_to_parent<'a>(tree: &Tree<RequiredElement<'a>>, node: usize, success: bool) -> Self {
        let (data, parent) = tree.get_data_and_parent(node);
        ProcessingResult { result: ProcessingPredecessorResult::Child(success, data.get_child_index()), next_node: parent }
    }

    pub fn new_to_child<'a>(tree: &Tree<RequiredElement<'a>>, node: usize, child_index: usize) -> Self {
        let Some(child) = tree.get_children(node).get(child_index) else {
            on_error("Failed to get child node", EXIT_CODE_OTHER_ERROR);
        };
        ProcessingResult { result: ProcessingPredecessorResult::Parent, next_node: Some(*child) }
    }
}

//The main processing function. We look at the current node and delegate to the respective processing function based on the type of node.
#[inline(always)]
fn process_contrary(framework: &AbaFramework, contrary: usize) -> Solutions {
    
    let (number_of_assumptions, number_of_literals) = framework.get_count();
    let mut solutions = Solutions::new(number_of_assumptions, number_of_literals);
    if framework.literal_is_assumption(contrary) {
        solutions.add_assumption(contrary);
        solutions.commit();
        return solutions;
    }

    debug_println!("Starting tree processing for contrary {}", framework.get_name_for_literal(contrary));
    let mut tree = Tree::new(RequiredElement::LiteralInitial {child_index: 0, literal: contrary });
    let mut forbidden_literals = HashSet::new();
    let mut node = 0;
    let mut predecessor_result  = ProcessingPredecessorResult::Parent;

    //The main loop for processing
    loop{
        let data = tree.get(node);
        let result = match data {
            RequiredElement::LiteralInitial { .. } => process_literal_initial(framework, &mut solutions, &mut tree, node, &mut forbidden_literals, predecessor_result),
            RequiredElement::LiteralSkipped { .. } => process_literal_skipped(&mut tree, node, predecessor_result),
            RequiredElement::LiteralAdded { .. } => process_literal_added(framework, &mut solutions, &mut tree, node, &mut forbidden_literals, predecessor_result),
            RequiredElement::Assumption { .. } => process_assumption(&mut solutions, &mut tree, node, predecessor_result)
        };

        //If there is no next node, i.e. we returned from the root node, we are finished
        if let Some(next_node) = result.next_node {
            node = next_node;
            predecessor_result = result.result;
        } else {
            break;
        }
    }
   
    solutions 
}

#[inline(always)]
fn process_literal_initial<'a, 'b: 'a>(framework: &'b AbaFramework, solutions: &mut Solutions, tree: &mut Tree<RequiredElement<'a>>, node: usize, forbidden_literals: &mut HashSet<usize>, predecessor_result: ProcessingPredecessorResult) -> ProcessingResult {    
    let RequiredElement::LiteralInitial { literal, .. } = tree.get(node) else {
        on_error("Got wrong node type for literal initial", EXIT_CODE_OTHER_ERROR);
    };
    let literal = *literal;
    debug_println!("Literal {} initial", framework.get_name_for_literal(literal));

    let ProcessingPredecessorResult::Parent = predecessor_result else {
        on_error("Tried to access literal inital from other than parent", EXIT_CODE_OTHER_ERROR);
    };

    let Some(head_rules) = framework.get_head_rules_for_literal(literal) else {
        return ProcessingResult::new_to_parent(tree, node, false);
    };

    let data = tree.get_mut(node);

    if !solutions.add_literal(literal) { //If the literal is already contained in the candidate soluition, we do not expand it again
        debug_println!("Skipping literal {}", framework.get_name_for_literal(literal));
        *data = RequiredElement::LiteralSkipped { child_index: data.get_child_index(), literal };
        return ProcessingResult::new_to_parent(tree, node, true);
    } else {
        debug_println!("Added literal {}", framework.get_name_for_literal(literal));
        *data = RequiredElement::LiteralAdded { child_index: data.get_child_index(), literal, head_rules, next_rule_index: 0 , children_count: 0, next_child_index: 0 };
    }

    let head_rules_count = head_rules.len();
    for i in 0 .. head_rules_count {
        if let Some(children) = get_children_for_head_rule(framework, head_rules, i, forbidden_literals) {
            let data = tree.get_mut(node);
            *data = RequiredElement::LiteralAdded { child_index: data.get_child_index(), literal, head_rules, next_rule_index: i + 1 , children_count: children.len(), next_child_index: 0 };
            if children.is_empty() { 
                debug_println!("No children for literal {}", framework.get_name_for_literal(literal));
                
                if tree.get_parent(node).is_none() { 
                    debug_println!("Visting root literal for empty child -> committing solution");
                    solutions.commit(); //We are the root node and our immediate child on initial visit is derivable -> commit solution as any other solution will be subsumed
                }
                return ProcessingResult::new_to_parent(tree, node, true);

            } else {
                debug_print!("Adding children for literal {}: ", framework.get_name_for_literal(literal));
                for child in children {
                    debug_print!("{}, ", framework.get_name_for_literal(child.get_literal_index()));
                    tree.add_child(node, child);
                }
                debug_println!("");
                let insertion_result = forbidden_literals.insert(literal); //We cannot use a literal to derive itself
                if !insertion_result {
                    on_error("Literal initial on blacklist", EXIT_CODE_OTHER_ERROR);
                };
                return ProcessingResult::new_to_child(tree, node, 0);
            }
        }
    }
    
    debug_println!("Failed for literal {}", framework.get_name_for_literal(literal));
    return ProcessingResult::new_to_parent(tree, node, false);
     
}

#[inline(always)]
fn process_literal_skipped<'a>(tree: &mut Tree<RequiredElement>, node: usize, predecessor_result: ProcessingPredecessorResult) -> ProcessingResult {
    let ProcessingPredecessorResult::Parent = predecessor_result else {
        on_error("Tried to access skipped literal from other than parent", EXIT_CODE_OTHER_ERROR);
    };
    debug_println!("Ignoring skipped literal");
    ProcessingResult::new_to_parent(tree, node, false)}

#[inline(always)]
fn process_literal_added<'a>(framework: &AbaFramework, solutions: &mut Solutions, tree: &mut Tree<RequiredElement>, node: usize, forbidden_literals: &mut HashSet<usize>, predecessor_result: ProcessingPredecessorResult) -> ProcessingResult {

    #[cfg(debug_assertions)] if node == 0 { 
        debug_println!("Back at root node"); 
    }

    //Coming from parent -> try last child again
    if let ProcessingPredecessorResult::Parent = predecessor_result { 
        let RequiredElement::LiteralAdded { literal, children_count, ..} = tree.get(node) else {
            on_error("Got wrong node type for literal added in parent case", EXIT_CODE_OTHER_ERROR);
        };
        let insertion_result = forbidden_literals.insert(*literal);
        if !insertion_result {
            on_error("Literal added on blacklist", EXIT_CODE_OTHER_ERROR);
        };
        debug_println!("Visting literal {} for parent -> delegating to last child", framework.get_name_for_literal(*literal));
        if *children_count == 0 { //AG TODO: Fix for empty rules
            return ProcessingResult::new_to_parent(tree, node, false);
        } else {
            return ProcessingResult::new_to_child(tree, node, *children_count - 1);
        }
    }

    //Coming from child that was successfull -> try next, go back to parent or commit
    if let ProcessingPredecessorResult::Child(true, child_index) = predecessor_result {
        let RequiredElement::LiteralAdded {literal, children_count, ..} = tree.get(node) else {
            on_error("Got wrong node type for literal added in successfull child case", EXIT_CODE_OTHER_ERROR);
        };
        
        let next_child_index = child_index + 1;
        if next_child_index < *children_count {
            debug_println!("Visting literal {} for successful child -> delegating to next child", framework.get_name_for_literal(*literal));
            return ProcessingResult::new_to_child(tree, node, next_child_index);
        } else {
            if tree.get_parent(node).is_none() { 
                debug_println!("Visting literal {} for successful child -> committing solution", framework.get_name_for_literal(*literal));
                solutions.commit(); //We are the root node and our last child returned success -> commit solution and backtrack children
                return ProcessingResult::new_to_child(tree, node, child_index);
            } else {
                let removal_result = forbidden_literals.remove(literal);
                if !removal_result {
                    on_error("Literal (success) not on blacklist", EXIT_CODE_OTHER_ERROR);
                };
                debug_println!("Propagating success from literal {} to parent", framework.get_name_for_literal(*literal));
                return ProcessingResult::new_to_parent(tree, node, true);
            }
        }
    }

    //Coming from child that failed
    let ProcessingPredecessorResult::Child(false, child_index) = predecessor_result else {
        on_error("Got wrong node type for literal added", EXIT_CODE_OTHER_ERROR);
    };

    //Retry previous (successfull) child
    if child_index > 0 {
        debug_println!("Visting literal from failed child -> retry last successfull child");
        tree.undo_child(node, child_index, |value| undo_value(solutions, value));
        return ProcessingResult::new_to_child(tree, node, child_index - 1);
    }    

    //All childs of the current rule have failed -> try next rule if possible unless current candidate solution is already contained
    tree.undo_child(node, 0, |value| undo_value(solutions, value));
    tree.remove_children_subtree(node);

    let RequiredElement::LiteralAdded {literal, head_rules, next_rule_index, children_count, next_child_index, .. } = tree.get_mut(node) else {
        on_error("Got wrong node type for literal added", EXIT_CODE_OTHER_ERROR);
    };

    //If the current candiate solution is already contained, we need to backtrack to the last added assumption
    if solutions.is_candidate_contained() {
        let removal_result = forbidden_literals.remove(literal);
        if !removal_result {
            on_error("Literal (failed) not on blacklist", EXIT_CODE_OTHER_ERROR);
        };
        debug_println!("Premature stop for literal {} as candidate solution is already contained -> back to parent", framework.get_name_for_literal(*literal));
        return ProcessingResult::new_to_parent(tree, node, false);
    }

    //Expand next rule if there exists one. Otherwise, backtrack to parent
    loop {
        if *next_rule_index == head_rules.len() {
            let removal_result = forbidden_literals.remove(literal);
            if !removal_result {
                on_error("Literal (failed) not on blacklist", EXIT_CODE_OTHER_ERROR);
            };
            debug_println!("Failed to find new rule for literal {} -> back to parent", framework.get_name_for_literal(*literal));
            return ProcessingResult::new_to_parent(tree, node, false);
        }
        
        let children = get_children_for_head_rule(framework, head_rules, *next_rule_index, forbidden_literals);
        *next_rule_index += 1;

        if let Some(children) = children {
            debug_print!("Adding children for literal {}: ", framework.get_name_for_literal(*literal));
            *next_child_index = 0;
            *children_count = children.len();
            for child in children {
                debug_print!("{}, ", child.get_literal_index());
                tree.add_child(node, child);
            }
            debug_println!("");
            return ProcessingResult::new_to_child(tree, node, 0); //Continue with first child
        }

    }
}

#[inline(always)]
fn get_children_for_head_rule<'a>(framework: &AbaFramework, head_rules: &Box<[usize]>, rule_index: usize, forbidden_literals: &mut HashSet<usize>) -> Option<Vec<RequiredElement<'a>>> {
    let Some(head_rule) = head_rules.get(rule_index) else {
        on_error("Failed to get next rule", EXIT_CODE_OTHER_ERROR);
    };
    let mut children = Vec::new();
    
    let head_rule = framework.get_rule(*head_rule);
    for assumption in head_rule.get_body_assumptions() {
        children.push(RequiredElement::Assumption { child_index: children.len(), assumption: *assumption, first_pass: true, was_added: false });
    }
    
    for literal in head_rule.get_body_literals() {
        if forbidden_literals.contains(literal) {
            return None;
        }
        children.push(RequiredElement::LiteralInitial { child_index: children.len(), literal: *literal });
    }

    Some(children)
}

#[inline(always)]
fn process_assumption<'a>(solutions: &mut Solutions, tree: &mut Tree<RequiredElement>, node: usize, predecessor_result: ProcessingPredecessorResult) -> ProcessingResult {
    let RequiredElement::Assumption { assumption, first_pass, was_added, .. } = tree.get_mut(node) else {
        on_error("Got wrong node type for assumption", EXIT_CODE_OTHER_ERROR);
    };

    match predecessor_result {
        ProcessingPredecessorResult::Parent => {
            if *first_pass {
                *first_pass = false;
                return match solutions.add_assumption(*assumption) {
                    solutions::AssumptionAddResult::Ok =>{
                        *was_added = true;
                        debug_println!("First pass for assumption {assumption} -> added");
                        ProcessingResult::new_to_parent(tree, node, true)
                    }
                    solutions::AssumptionAddResult::Subsumed => {
                        debug_println!("First pass for assumption {assumption} -> subsumed");
                        ProcessingResult::new_to_parent(tree, node, false) 
                    },
                    solutions::AssumptionAddResult::AlreadyExists => {
                        debug_println!("First pass for assumption {assumption} -> already exists");
                        *was_added = false;
                        ProcessingResult::new_to_parent(tree, node, true)
                    }
                }
            }
    
            debug_println!("Second pass for assumption {assumption} -> removed");
            return ProcessingResult::new_to_parent(tree, node, false);
        }
        ProcessingPredecessorResult::Child(success, ..) => {
            debug_println!("Propagating success from assumption {assumption} to parent");
            ProcessingResult::new_to_parent(tree, node, success)
        }
    }
}

#[inline(always)]
fn undo_value<'a>(solutions: &mut Solutions, value: &mut RequiredElement<'a>) -> RequiredElement<'a> {
    match value {
        RequiredElement::LiteralInitial { child_index, literal } => { RequiredElement::LiteralInitial { child_index: *child_index, literal: *literal } },
        RequiredElement::LiteralSkipped { child_index, literal } => { RequiredElement::LiteralInitial { child_index: *child_index, literal: *literal } },
        RequiredElement::LiteralAdded { child_index, literal, .. } => { 
            solutions.remove_literal(*literal);
            RequiredElement::LiteralInitial { child_index: *child_index, literal: *literal } },
        RequiredElement::Assumption { child_index, was_added, assumption, ..} => { 
            if *was_added { solutions.remove_assumption(*assumption) }
            RequiredElement::Assumption { child_index: *child_index, assumption: *assumption, first_pass: true, was_added: false } }
    }
}