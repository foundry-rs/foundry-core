export function isUserRejection(error: unknown): boolean {
  if (typeof error !== "object" || !error) return false;
  const candidate = error as { code?: unknown; cause?: unknown };
  return candidate.code === 4001 || isUserRejection(candidate.cause);
}
