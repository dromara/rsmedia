use std::ptr;

use super::{Format, Input, Output};
use rsmpeg::ffi;

pub struct Iter {
    input: *mut ffi::AVInputFormat,
    output: *mut ffi::AVOutputFormat,
    step: Step,
}

enum Step {
    Input,
    Output,
    Done,
}

impl Iter {
    pub fn new() -> Self {
        Iter {
            input: ptr::null_mut(),
            output: ptr::null_mut(),
            step: Step::Input,
        }
    }
}

impl Default for Iter {
    fn default() -> Self {
        Self::new()
    }
}

impl Iterator for Iter {
    type Item = Format;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        unsafe {
            match self.step {
                Step::Input => {
                    let ptr = ffi::av_iformat_next(self.input);

                    if ptr.is_null() && !self.input.is_null() {
                        self.step = Step::Output;

                        self.next()
                    } else {
                        self.input = ptr;

                        Some(Format::Input(Input::wrap(ptr)))
                    }
                }

                Step::Output => {
                    let ptr = ffi::av_oformat_next(self.output);

                    if ptr.is_null() && !self.output.is_null() {
                        self.step = Step::Done;

                        self.next()
                    } else {
                        self.output = ptr;

                        Some(Format::Output(Output::wrap(ptr)))
                    }
                }

                Step::Done => None,
            }
        }
    }
}
