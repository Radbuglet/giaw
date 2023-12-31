pub fn ensure_index<T: Default>(vec: &mut Vec<T>, index: usize) -> &mut T {
    ensure_length(vec, index + 1);
    &mut vec[index]
}

fn ensure_length<T: Default>(vec: &mut Vec<T>, length: usize) {
    if vec.len() < length {
        vec.resize_with(length, Default::default);
    }
}
