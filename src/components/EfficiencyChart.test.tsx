import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { EfficiencyMetricEvaluation } from "../types";
import { EfficiencyChart } from "./EfficiencyChart";

const metrics: EfficiencyMetricEvaluation[] = [
  {
    key: "serverMetricA",
    label: "Backend label A",
    question: "Did A happen?",
    higherProbabilityIsBetter: true,
    probability: 0.91,
    score: 91,
  },
  {
    key: "serverMetricB",
    label: "Backend label B",
    question: "Did undesirable B happen?",
    higherProbabilityIsBetter: false,
    probability: 0.27,
    score: 73,
  },
];

describe("EfficiencyChart", () => {
  it("renders backend-supplied metric labels and questions without a frontend mapping", () => {
    render(<EfficiencyChart metrics={metrics} />);

    for (const metric of metrics) {
      expect(screen.getByText(metric.label)).toBeInTheDocument();
      const trigger = screen.getByRole("button", { name: `About ${metric.label}` });
      const tooltipId = trigger.getAttribute("aria-describedby");

      expect(tooltipId).toBe(`efficiency-metric-tip-${metric.key}`);
      expect(document.getElementById(tooltipId!)).toHaveTextContent(metric.question);
    }
    expect(document.getElementById("efficiency-metric-tip-serverMetricB")).toHaveTextContent(
      "reverses Jev's probability",
    );
  });

  it("asks for re-analysis when an old cache has no linked metric definitions", () => {
    render(<EfficiencyChart metrics={[]} />);

    expect(screen.getByText("Re-analyse to view Jev metrics.")).toBeInTheDocument();
  });
});
