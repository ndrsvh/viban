import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { ConfirmDialog } from "@/components/confirm-dialog";

describe("ConfirmDialog", () => {
  it("renders nothing when closed", () => {
    render(
      <ConfirmDialog
        open={false}
        title="Merge it?"
        description="Detail"
        confirmLabel="Merge"
        busy={false}
        onConfirm={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );
    expect(screen.queryByText("Merge it?")).not.toBeInTheDocument();
  });

  it("shows the title and description and confirms", async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();
    render(
      <ConfirmDialog
        open
        title="Merge it?"
        description="This merges the branch into the project."
        confirmLabel="Merge"
        busy={false}
        onConfirm={onConfirm}
        onOpenChange={vi.fn()}
      />,
    );
    expect(screen.getByText("Merge it?")).toBeInTheDocument();
    expect(
      screen.getByText("This merges the branch into the project."),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Merge" }));
    expect(onConfirm).toHaveBeenCalled();
  });

  it("cancels via the cancel button", async () => {
    const user = userEvent.setup();
    const onOpenChange = vi.fn();
    render(
      <ConfirmDialog
        open
        title="T"
        description="D"
        confirmLabel="Go"
        busy={false}
        onConfirm={vi.fn()}
        onOpenChange={onOpenChange}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("shows the busy label and disables both buttons while busy", () => {
    render(
      <ConfirmDialog
        open
        title="T"
        description="D"
        confirmLabel="Go"
        busyLabel="Going…"
        busy
        onConfirm={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: "Going…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
  });
});
