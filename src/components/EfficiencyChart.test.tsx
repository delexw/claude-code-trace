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
  {
    key: "serverRubricC",
    label: "Backend label C",
    question: "How much C happened?",
    higherProbabilityIsBetter: true,
    probability: 0.64,
    score: 30,
    scale: {
      levels: [
        "Far too little C",
        "A little less C",
        "C matched",
        "A little more C",
        "Far too much C",
      ],
      position: 3.4,
      level: "A little more C",
    },
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

  it("names the rubric level Jev picked so the direction is visible, not just the bar", () => {
    render(<EfficiencyChart metrics={metrics} />);

    const tooltip = document.getElementById("efficiency-metric-tip-serverRubricC");

    expect(tooltip).toHaveTextContent("Jev rated this “A little more C”");
    expect(tooltip).toHaveTextContent("how close that is to “C matched”");
    expect(tooltip).not.toHaveTextContent("reverses Jev's probability");
  });

  it("asks for re-analysis when an old cache has no linked metric definitions", () => {
    render(<EfficiencyChart metrics={[]} />);

    expect(screen.getByText("Re-analyse to view Jev metrics.")).toBeInTheDocument();
  });
});
