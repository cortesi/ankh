import { SHARED_RESERVED_NAMES } from "./generated-names";

const RESERVED_NAMESPACE_NAMES: ReadonlySet<string> = new Set(SHARED_RESERVED_NAMES);

/** Whether `name` is reserved by Ankh's shared list or by the product's `productReserved`. */
function isReserved(name: string, productReserved: readonly string[]) {
  return RESERVED_NAMESPACE_NAMES.has(name) || productReserved.includes(name);
}

export const PASSWORD_MIN_LENGTH = 8;

export function normalizeName(value: string) {
  return value.trim().toLowerCase();
}

export function validateEmail(value: string) {
  const normalized = value.trim().toLowerCase();
  if (!normalized) {
    return "Email is required";
  }
  if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(normalized)) {
    return "Invalid email address";
  }
  return null;
}

export function validateLoginIdentifier(value: string) {
  if (!value.trim()) {
    return "Email or username is required";
  }
  if (value.includes("@")) {
    return validateEmail(value);
  }
  return null;
}

export function validatePassword(value: string) {
  if (value.length < PASSWORD_MIN_LENGTH) {
    return "Password too short";
  }
  return null;
}

/** Validate a username. `productReserved` adds the product's reserved names to the shared list. */
export function validateUsername(value: string, productReserved: readonly string[] = []) {
  const normalized = normalizeName(value);
  const formatError = validateNameFormat(normalized);
  if (formatError) {
    return formatError;
  }
  if (isReserved(normalized, productReserved)) {
    return "This username is reserved";
  }
  return null;
}

/** Validate an organization name. `productReserved` is as for `validateUsername`. */
export function validateOrgName(value: string, productReserved: readonly string[] = []) {
  const normalized = normalizeName(value);
  const formatError = validateNameFormat(normalized);
  if (formatError) {
    return formatError;
  }
  if (isReserved(normalized, productReserved)) {
    return "This name is reserved";
  }
  return null;
}

function validateNameFormat(value: string) {
  if (value.length < 3) {
    return "must be at least 3 characters";
  }
  if (value.length > 39) {
    return "must be at most 39 characters";
  }
  if (!/^[a-z0-9-]+$/.test(value)) {
    return "must contain only lowercase letters, numbers, and hyphens";
  }
  if (!/^[a-z0-9]/.test(value) || !/[a-z0-9]$/.test(value)) {
    return "must start and end with a letter or number";
  }
  if (value.includes("--")) {
    return "cannot contain consecutive hyphens";
  }
  return null;
}
