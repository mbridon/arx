use crate::common::*;
use jubako as jbk;
use std::path::{Path, PathBuf};
use std::rc::Rc;

struct Lister {}

impl ArxOperator for Lister {
    fn on_start(&self, _current_path: &mut PathBuf) -> jbk::Result<()> {
        Ok(())
    }

    fn on_stop(&self, _current_path: &mut PathBuf) -> jbk::Result<()> {
        Ok(())
    }

    fn on_file(&self, current_path: &mut PathBuf, entry: &Entry) -> jbk::Result<()> {
        current_path.push(entry.get_path()?);
        println!("{}", current_path.display());
        current_path.pop();
        Ok(())
    }

    fn on_link(&self, current_path: &mut PathBuf, entry: &Entry) -> jbk::Result<()> {
        current_path.push(entry.get_path()?);
        println!("{}", current_path.display());
        current_path.pop();
        Ok(())
    }

    fn on_directory_enter(&self, current_path: &mut PathBuf, entry: &Entry) -> jbk::Result<()> {
        current_path.push(entry.get_path()?);
        println!("{}", current_path.display());
        Ok(())
    }

    fn on_directory_exit(&self, current_path: &mut PathBuf, _entry: &Entry) -> jbk::Result<()> {
        current_path.pop();
        Ok(())
    }
}

pub fn list<P: AsRef<Path>>(infile: P) -> jbk::Result<()> {
    let arx = Arx::new(infile)?;
    let mut runner = ArxRunner::new(&arx, PathBuf::with_capacity(2048));

    let index = arx.get_index_for_name("root")?;
    let resolver = jbk::reader::Resolver::new(Rc::clone(arx.get_value_storage()));
    let op = Lister {};
    runner.run(index.get_finder(arx.get_entry_storage(), resolver)?, &op)
}
