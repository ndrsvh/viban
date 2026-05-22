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
        onWorkWithoutGit={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );
    expect(
      screen.queryByText("Set up git for this project?"),
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
        onWorkWithoutGit={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Initialize git" }));
    expect(onConfirm).toHaveBeenCalled();
  });

  it("starts a session without git", async () => {
    const user = userEvent.setup();
    const onWorkWithoutGit = vi.fn();
    render(
      <GitInitDialog
        open
        busy={false}
        onConfirm={vi.fn()}
        onWorkWithoutGit={onWorkWithoutGit}
        onOpenChange={vi.fn()}
      />,
    );
    await user.click(
      screen.getByRole("button", { name: "Work without Git" }),
    );
    expect(onWorkWithoutGit).toHaveBeenCalled();
  });

  it("cancels via the cancel button", async () => {
    const user = userEvent.setup();
    const onOpenChange = vi.fn();
    render(
      <GitInitDialog
        open
        busy={false}
        onConfirm={vi.fn()}
        onWorkWithoutGit={vi.fn()}
        onOpenChange={onOpenChange}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("disables every button while an action is running", () => {
    render(
      <GitInitDialog
        open
        busy
        onConfirm={vi.fn()}
        onWorkWithoutGit={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );
    expect(
      screen.getByRole("button", { name: "Initializing…" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Work without Git" }),
    ).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
  });
});
