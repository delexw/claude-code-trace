import {
  BarElement,
  CategoryScale,
  Chart as ChartJS,
  LinearScale,
  Tooltip,
  type ChartOptions,
} from "chart.js";
import { Bar } from "react-chartjs-2";
import { VscInfo } from "react-icons/vsc";
import { colors } from "../lib/theme";
import type { EfficiencyMetricEvaluation } from "../types";

ChartJS.register(CategoryScale, LinearScale, BarElement, Tooltip);

interface EfficiencyChartProps {
  metrics: EfficiencyMetricEvaluation[];
}

export function EfficiencyChart({ metrics }: EfficiencyChartProps) {
  const data = {
    labels: metrics.map(({ label }) => label),
    datasets: [
      {
        data: metrics.map(({ score }) => score),
        backgroundColor: colors.accent,
        borderRadius: 3,
      },
    ],
  };
  const options: ChartOptions<"bar"> = {
    indexAxis: "y",
    responsive: true,
    maintainAspectRatio: false,
    animation: false,
    plugins: { legend: { display: false } },
    scales: {
      x: {
        min: 0,
        max: 100,
        ticks: { color: colors.textSecondary },
        grid: { color: colors.bgElevated },
        border: { color: colors.border },
      },
      y: {
        ticks: { display: false },
        grid: { display: false },
        border: { display: false },
      },
    },
  };
  if (metrics.length === 0) {
    return <div className="efficiency-chart__unavailable">Re-analyse to view Jev metrics.</div>;
  }
  return (
    <div className="efficiency-chart" aria-label="Jev metric evaluations chart">
      <div
        className="efficiency-chart__labels"
        style={{ gridTemplateRows: `repeat(${metrics.length}, minmax(0, 1fr))` }}
      >
        {metrics.map((metric) => {
          const tooltipId = `efficiency-metric-tip-${metric.key}`;
          return (
            <div className="efficiency-chart__label" key={metric.key}>
              <span>{metric.label}</span>
              <span className="efficiency-chart__tip">
                <button
                  type="button"
                  className="efficiency-chart__tip-trigger"
                  aria-label={`About ${metric.label}`}
                  aria-describedby={tooltipId}
                >
                  <VscInfo aria-hidden="true" />
                </button>
                <span id={tooltipId} role="tooltip" className="efficiency-chart__tooltip">
                  Jev question: “{metric.question}”{" "}
                  {metric.higherProbabilityIsBetter
                    ? "Higher is better."
                    : "This bar reverses Jev's probability so higher is better."}
                </span>
              </span>
            </div>
          );
        })}
      </div>
      <div className="efficiency-chart__plot">
        <Bar data={data} options={options} />
      </div>
    </div>
  );
}
