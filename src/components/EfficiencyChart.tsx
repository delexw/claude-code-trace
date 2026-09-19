import {
  BarElement,
  CategoryScale,
  Chart as ChartJS,
  LinearScale,
  Tooltip,
  type ChartOptions,
} from "chart.js";
import { Bar } from "react-chartjs-2";
import { colors } from "../lib/theme";
import type { SessionEfficiencyAnalysis } from "../types";

ChartJS.register(CategoryScale, LinearScale, BarElement, Tooltip);

interface EfficiencyChartProps {
  dimensions: SessionEfficiencyAnalysis["dimensions"];
}

export function efficiencyDimensionValues(
  dimensions: SessionEfficiencyAnalysis["dimensions"],
): number[] {
  return [
    dimensions.progress,
    dimensions.toolUse,
    dimensions.focus,
    dimensions.exploration,
    dimensions.recovery,
    dimensions.tokenUse,
  ];
}

export function EfficiencyChart({ dimensions }: EfficiencyChartProps) {
  const data = {
    labels: ["Progress", "Tool use", "Focus", "Exploration", "Recovery", "Token use"],
    datasets: [
      {
        data: efficiencyDimensionValues(dimensions),
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
        ticks: { color: colors.textPrimary },
        grid: { display: false },
        border: { display: false },
      },
    },
  };
  return (
    <div className="efficiency-chart" aria-label="Efficiency dimensions chart">
      <Bar data={data} options={options} />
    </div>
  );
}
