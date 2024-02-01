use std::{
    io::Read,
    ops::DerefMut,
    path::{Path, PathBuf},
};

use drop_guard::guard;

pub fn use_path<'b>(
    path: &'b mut PathBuf,
    comps: &[&Path],
) -> impl DerefMut<Target = &'b mut PathBuf> + 'b {
    let mut added = 0;

    for part in comps {
        assert_eq!(part.components().count(), 1);
        assert!(part.is_relative());

        path.push(part);
        added += 1;
    }

    guard(path, move |path| {
        for _ in 0..added {
            path.pop();
        }
    })
}

pub fn read_vec(f: &mut impl Read) -> anyhow::Result<Vec<u8>> {
    let mut v = Vec::new();
    f.read_to_end(&mut v)?;
    Ok(v)
}
