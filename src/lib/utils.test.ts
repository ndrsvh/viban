import { describe, expect, it } from "vitest";

import { cn } from "@/lib/utils";

describe("cn", () => {
  it("joins multiple class names", () => {
    expect(cn("a", "b", "c")).toBe("a b c");
  });

  it("ignores falsy values", () => {
    expect(cn("a", false, null, undefined, "", "b")).toBe("a b");
  });

  it("applies conditional object syntax", () => {
    expect(cn("base", { active: true, hidden: false })).toBe("base active");
  });

  it("flattens nested arrays", () => {
    expect(cn(["a", ["b", "c"]])).toBe("a b c");
  });

  it("merges conflicting tailwind utilities so the last one wins", () => {
    expect(cn("p-2", "p-4")).toBe("p-4");
    expect(cn("text-sm", "text-lg")).toBe("text-lg");
  });

  it("keeps non-conflicting tailwind utilities", () => {
    expect(cn("px-2", "py-4")).toBe("px-2 py-4");
  });

  it("returns an empty string when given nothing", () => {
    expect(cn()).toBe("");
  });
});
