import { describe, expect, it } from "vitest";
import { efficiencyDimensionValues } from "./EfficiencyChart";

describe("efficiencyDimensionValues", () => {
  it("includes token use as a first-class efficiency dimension", () => {
    expect(
      efficiencyDimensionValues({
        progress: 91,
        toolUse: 82,
        focus: 73,
        exploration: 64,
        recovery: 55,
        tokenUse: 46,
      }),
    ).toEqual([91, 82, 73, 64, 55, 46]);
  });
});
