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

/// A failed import wrote nothing. Display is a file row and a column, a
/// map-or-create account message, an identity-strategy note, or a kernel `code`.
#[derive(Debug)]
pub enum ImportError {
    Rows(Vec<RowFault>),
    /// Every listed file row failed to bind to exactly one usable account.
    UnmappedAccounts(Vec<u32>),
    Identity(crate::identity::IdentityError),
    Kernel(KernelError),
}

impl ImportError {
    pub fn rows(faults: Vec<RowFault>) -> Self {
        Self::Rows(faults)
    }

    pub fn kernel(err: KernelError) -> Self {
        Self::Kernel(err)
    }

    pub fn unmapped(rows: Vec<u32>) -> Self {
        Self::UnmappedAccounts(rows)
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
            Self::UnmappedAccounts(rows) => {
                for (index, row) in rows.iter().enumerate() {
                    if index > 0 {
                        f.write_str("; ")?;
                    }
                    write!(
                        f,
                        "row={row} column=Account: map this account to an existing financial account or create one"
                    )?;
                }
                Ok(())
            }
            Self::Identity(err) => write!(f, "{err}"),
            Self::Kernel(err) => write!(f, "code={}", err.code.as_str()),
        }
    }
}

impl std::error::Error for ImportError {}
