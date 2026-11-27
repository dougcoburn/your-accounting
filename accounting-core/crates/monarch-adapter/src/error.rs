//! Adapter faults name a file row and a column. They do not carry cell text.

use std::fmt;

use accounting_kernel::v2::Error as KernelError;

/// Monarch export columns, in file order.
pub const COLUMNS: [&str; 14] = [
    "Date",
    "Merchant",
    "Category",
    "Account",
    "Original Statement",
    "Notes",
    "Amount",
    "Tags",
    "Owner",
    "Reviewed",
    "Reviewed At",
    "Reviewed By",
    "Pending",
    "Id",
];

pub fn column_at(index: usize) -> &'static str {
    COLUMNS.get(index).copied().unwrap_or("Id")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowFault {
    /// 1-based physical line where the record starts.
    pub row: u32,
    pub column: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDiff {
    pub row: u32,
    pub columns: Vec<&'static str>,
}

/// A failed import wrote nothing. Display is `row` and `column`, or a kernel `code`.
#[derive(Debug)]
pub enum ImportError {
    Rows(Vec<RowFault>),
    Kernel(KernelError),
}

impl ImportError {
    pub fn rows(faults: Vec<RowFault>) -> Self {
        Self::Rows(faults)
    }

    pub fn kernel(err: KernelError) -> Self {
        Self::Kernel(err)
    }
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rows(faults) => {
                for (index, fault) in faults.iter().enumerate() {
                    if index > 0 {
                        f.write_str("; ")?;
                    }
                    write!(f, "row={} column={}", fault.row, fault.column)?;
                }
                Ok(())
            }
            Self::Kernel(err) => write!(f, "code={}", err.code.as_str()),
        }
    }
}

impl std::error::Error for ImportError {}
