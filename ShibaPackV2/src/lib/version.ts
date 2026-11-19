/** 简单的版本号比较：数字段按数值比较，其余按字符串比较。 */
function tokens(value: string): (number | string)[] {
  return value
    .trim()
    .toLowerCase()
    .split(/[._\-+]/)
    .flatMap((part) => part.split(/(\d+)/))
    .filter((part) => part !== "")
    .map((part) => (/^\d+$/.test(part) ? Number(part) : part));
}

export function compareVersions(a: string, b: string): number {
  const left = tokens(a);
  const right = tokens(b);
  const length = Math.max(left.length, right.length);
  for (let index = 0; index < length; index += 1) {
    const x = left[index];
    const y = right[index];
    if (x === undefined) return -1;
    if (y === undefined) return 1;
    if (x === y) continue;
    if (typeof x === "number" && typeof y === "number") return x > y ? 1 : -1;
    return String(x) > String(y) ? 1 : -1;
  }
  return 0;
}
