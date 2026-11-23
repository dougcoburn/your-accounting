/** Parse a decimal dollar amount into signed minor units (cents). Avoids float math. */
export function parseMinor(raw: string): number {
  const trimmed = raw.trim().replace(/[$,]/g, "");
  if (!trimmed) throw new Error("empty amount");
  let negative = false;
  let body = trimmed;
  if (body.startsWith("(") && body.endsWith(")")) {
    negative = true;
    body = body.slice(1, -1).trim();
  } else if (body.startsWith("-")) {
    negative = true;
    body = body.slice(1);
  } else if (body.startsWith("+")) {
    body = body.slice(1);
  }
  if (!/^\d+(\.\d+)?$/.test(body)) {
    throw new Error("invalid amount");
  }
  const [wholeRaw, fracRaw = ""] = body.split(".");
  const whole = BigInt(wholeRaw || "0");
  const frac = (fracRaw + "00").slice(0, 2);
  const minor = whole * 100n + BigInt(frac);
  const asNumber = Number(minor);
  if (!Number.isSafeInteger(asNumber)) throw new Error("amount overflow");
  return negative ? -asNumber : asNumber;
}

export function formatUsd(minor: number): string {
  const sign = minor < 0 ? "-" : "";
  const abs = Math.abs(minor);
  const whole = Math.floor(abs / 100);
  const cents = String(abs % 100).padStart(2, "0");
  const withCommas = String(whole).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return `${sign}$${withCommas}.${cents}`;
}

export function formatSignedUsd(minor: number): string {
  if (minor > 0) return `+${formatUsd(minor)}`;
  return formatUsd(minor);
}

/** Split a total into parts that sum exactly, putting leftover cents on the first part. */
export function splitTotal(totalMinor: number, weights: number[]): number[] {
  if (weights.length === 0) return [];
  const weightSum = weights.reduce((a, b) => a + b, 0);
  if (weightSum <= 0) throw new Error("weights must be positive");
  const parts = weights.map((w) => Math.floor((totalMinor * w) / weightSum));
  let remainder = totalMinor - parts.reduce((a, b) => a + b, 0);
  // Distribute leftover cents starting at index 0.
  let i = 0;
  while (remainder !== 0) {
    const step = remainder > 0 ? 1 : -1;
    parts[i % parts.length] += step;
    remainder -= step;
    i += 1;
  }
  return parts;
}
