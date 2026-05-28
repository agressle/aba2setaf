#[derive(Clone, Debug)]
pub struct AbaRule {
    head: usize,
    number_of_assumptions: usize,
    body_literals: Box<[usize]>
}

impl AbaRule {

    #[inline(always)]
    pub fn new(head: usize, mut body_assumptions: Vec<usize>, body_literals: Vec<usize>) -> AbaRule {
        let number_of_assumptions = body_assumptions.len();
        body_assumptions.extend(body_literals);
        AbaRule {
            head,
            number_of_assumptions,
            body_literals: body_assumptions.into_boxed_slice()
        }
    }
    
    #[inline(always)]
    pub fn get_head(&self) -> usize {
        self.head
    }

    #[inline(always)]
    pub fn get_body_assumptions(&self) -> &[usize] {
        &self.body_literals[..self.number_of_assumptions]
    }
    
    #[inline(always)]
    pub fn get_body_literals(&self) -> &[usize] {
        &self.body_literals[self.number_of_assumptions..]
    }

    #[inline(always)]
    pub fn clear_body(&mut self) {
        self.body_literals = Box::new([]);
        self.number_of_assumptions = 0;
    }

    #[inline(always)]
    pub fn apply_mapping(&mut self, mapping: &Vec<usize>) {
        self.head = mapping[self.head];
        for literal in self.body_literals.iter_mut() {
            *literal = mapping[*literal];
        }
    }
}