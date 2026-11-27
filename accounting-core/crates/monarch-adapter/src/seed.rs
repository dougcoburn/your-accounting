//! Which Monarch category names become income or expense leaves.
//!
//! Transfer and payment names are absent on purpose, and so is Reimbursements.
//! Those stay hints on the observation. Ledger account type and parent cannot
//! be updated later, so a name that is not listed here is not created.

use accounting_kernel::v2::AccountType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryClass {
    pub name: String,
    pub account_type: AccountType,
}

pub const INCOME_PARENT: &str = "Income";
pub const EXPENSE_PARENT: &str = "Expense";

const INCOME: &[&str] = &[
    "Paychecks",
    "Interest",
    "Dividends",
    "Other Income",
    "Business Income",
    "Gifts Received",
];

const EXPENSE: &[&str] = &[
    "Groceries",
    "Restaurants & Bars",
    "Coffee Shops",
    "Gas",
    "Public Transit",
    "Taxi & Rideshare",
    "Auto Maintenance",
    "Medical",
    "Pharmacy",
    "Fitness",
    "Insurance",
    "Utilities",
    "Internet & Cable",
    "Phone",
    "Rent",
    "Mortgage",
    "Home Improvement",
    "Shopping",
    "Clothing",
    "Electronics",
    "Entertainment",
    "Subscriptions",
    "Travel",
    "Pets",
    "Personal Care",
    "Gifts",
    "Charity",
    "Education",
    "Child Care",
    "Taxes",
    "Fees",
];

/// Month-1 classification for the synthetic catalog. Reimbursements is omitted.
pub fn month1_category_classes() -> Vec<CategoryClass> {
    let mut out = Vec::with_capacity(INCOME.len() + EXPENSE.len());
    for name in INCOME {
        out.push(CategoryClass {
            name: (*name).to_string(),
            account_type: AccountType::Income,
        });
    }
    for name in EXPENSE {
        out.push(CategoryClass {
            name: (*name).to_string(),
            account_type: AccountType::Expense,
        });
    }
    out
}
