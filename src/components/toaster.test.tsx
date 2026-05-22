import { afterEach, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Toaster } from "@/components/toaster";
import { toast, useToastStore } from "@/stores/useToastStore";

afterEach(() => {
  useToastStore.setState({ toasts: [] });
});

describe("Toaster", () => {
  it("renders nothing when there are no toasts", () => {
    const { container } = render(<Toaster />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows an error toast and dismisses it", async () => {
    const user = userEvent.setup();
    render(<Toaster />);

    toast.error("worktree exploded");
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "worktree exploded",
    );

    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("stacks multiple toasts", async () => {
    render(<Toaster />);
    toast.error("first failure");
    toast.error("second failure");
    expect(await screen.findAllByRole("alert")).toHaveLength(2);
  });
});
