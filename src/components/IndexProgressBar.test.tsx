import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { IndexProgressBar } from "./IndexProgressBar";

describe("IndexProgressBar", () => {
  it("shows how far through the directory the backend is", () => {
    render(
      <IndexProgressBar
        progress={{
          files_read: 1240,
          total_files: 3375,
          bytes_read: 3_700_000,
          total_bytes: 10_000_000,
          done: false,
        }}
      />,
    );

    // The label and the bar both measure bytes. Counting files in the label while the
    // bar filled by bytes read as broken: "248 / 3,399" beside a bar nearly half full.
    expect(screen.getByText("1,240 sessions · 3.7 MB / 10.0 MB")).toBeInTheDocument();
    const bar = screen.getByRole("progressbar");
    expect(bar).toHaveAttribute("aria-valuenow", "3700000");
    expect(bar).toHaveAttribute("aria-valuemax", "10000000");
    expect(bar.querySelector(".index-progress__fill")).toHaveStyle({ width: "37%" });
  });

  it("says it is counting before the total is known", () => {
    // Counting the files takes milliseconds, but a bar reading 0 / 0 in that window
    // looks stuck rather than starting.
    render(
      <IndexProgressBar
        progress={{ files_read: 0, total_files: 0, bytes_read: 0, total_bytes: 0, done: false }}
      />,
    );

    expect(screen.getByText("Counting sessions...")).toBeInTheDocument();
    expect(screen.getByRole("progressbar").querySelector(".index-progress__fill")).toHaveStyle({
      width: "0%",
    });
  });

  it("empties its box once the walk has finished, without removing it", () => {
    render(
      <IndexProgressBar
        progress={{
          files_read: 3375,
          total_files: 3375,
          bytes_read: 10,
          total_bytes: 10,
          done: true,
        }}
      />,
    );

    // The box stays so the strip it sits in never changes height, but there is nothing
    // in it to read.
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
    expect(document.querySelector(".index-progress")).toBeEmptyDOMElement();
  });

  it("never reads over 100% when the directory grew while it was walked", () => {
    render(
      <IndexProgressBar
        progress={{
          files_read: 12,
          total_files: 10,
          bytes_read: 120,
          total_bytes: 100,
          done: false,
        }}
      />,
    );

    expect(screen.getByRole("progressbar").querySelector(".index-progress__fill")).toHaveStyle({
      width: "100%",
    });
  });
});
