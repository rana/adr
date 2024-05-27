use regex::Regex;

pub struct Prsr {
    // "46.86551919465073", "-96.83144324414937"
    pub re_flt: Regex,
}

impl Prsr {
    pub fn new() -> Self {
        Prsr {
            re_flt: Regex::new(r"^-?\d+\.\d+$").unwrap(),
        }
    }

    // pub fn edit(&self, lnes: &mut Vec<String>) {}
}
impl Default for Prsr {
    fn default() -> Self {
        Self::new()
    }
}
