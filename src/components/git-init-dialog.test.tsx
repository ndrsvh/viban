import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { GitInitDialog } from "@/components/git-init-dialog";

describe("GitInitDialog", () => {
  it("does not render anything when closed", () => {
    render(
      <GitInitDialog
        open={false}
        busy={false}
        onConfirm={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );
    expect(
      screen.queryByText("Initialize a git repository?"),
    ).not.toBeInTheDocument();
  });

  it("confirms initialization", async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();
    render(
      <GitInitDialog
        open
        busy={false}
        onConfirm={onConfirm}
        onOpenChange={vi.fn()}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Initialize git" }));
    expect(onConfirm).toHaveBeenCalled();
  });

  it("cancels via the cancel button", async () => {
    const user = userEvent.setup();
    const onOpenChange = vi.fn();
    render(
      <GitInitDialog
        open
        busy={false}
        onConfirm={vi.fn()}
        onOpenChange={onOpenChange}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("disables both buttons while initialization is running", () => {
    render(
      <GitInitDialog open busy onConfirm={vi.fn()} onOpenChange={vi.fn()} />,
    );
    expect(
      screen.getByRole("button", { name: "Initializing…" }),
    ).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
  });
});
