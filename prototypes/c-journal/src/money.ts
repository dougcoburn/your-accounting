export function parseSignedMinor(raw: string): number {
  let text = raw.trim();
  if (text === "") {
    throw new Error("empty amount");
  }
  let negative = false;
  if (text.startsWith("(") && text.endsWith(")")) {
    negative = true;
    text = text.slice(1, -1).trim();
  }
  if (text.startsWith("+")) {
    text = text.slice(1);
  } else if (text.startsWith("-")) {
    negative = !negative;
    text = text.slice(1);
  }
  text = text.replace(/[$,\s]/g, "");
  if (!/^\d+(\.\d{1,2})?$/.test(text)) {
    throw new Error("bad amount");
  }
  const [dollars, cents = ""] = text.split(".");
  const minor = Number(dollars) * 100 + Number(cents.padEnd(2, "0"));
  if (!Number.isSafeInteger(minor)) {
    throw new Error("amount overflow");
  }
  return negative ? -minor : minor;
}

export function addMinor(left: number, right: number): number {
  if (!Number.isSafeInteger(left) || !Number.isSafeInteger(right)) {
    throw new Error("amount overflow");
  }
  const sum = left + right;
  if (!Number.isSafeInteger(sum)) {
    throw new Error("amount overflow");
  }
  return sum;
}

export function formatMoney(minor: number): string {
  const negative = minor < 0;
  const abs = Math.abs(minor);
  const dollars = Math.floor(abs / 100);
  const cents = abs % 100;
  const grouped = String(dollars).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return `${negative ? "-" : ""}${grouped}.${String(cents).padStart(2, "0")}`;
}

export function formatSigned(minor: number): string {
  if (minor > 0) {
    return `+${formatMoney(minor)}`;
  }
  return formatMoney(minor);
}

export function minorToInput(minor: number): string {
  const negative = minor < 0;
  const abs = Math.abs(minor);
  const dollars = Math.floor(abs / 100);
  const cents = abs % 100;
  return `${negative ? "-" : ""}${dollars}.${String(cents).padStart(2, "0")}`;
}

export function formatStamp(iso: string): string {
  return iso.replace("T", " ").replace(/\.\d{3}Z$/, " UTC");
}

export function normalSide(accountType: string): "debit" | "credit" {
  return accountType === "asset" || accountType === "expense" ? "debit" : "credit";
}
