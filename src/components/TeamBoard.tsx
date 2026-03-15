import type { TeamSnapshot } from "../types";
import { getTeamColor } from "../lib/theme";
import { taskStatusIcons, ClaudeIcon, ForwardIcon } from "./Icons";

interface TeamBoardProps {
  teams: TeamSnapshot[];
}

export function TeamBoard({ teams }: TeamBoardProps) {
  // Filter out deleted teams
  const activeTeams = teams.filter((t) => !t.deleted);

  if (activeTeams.length === 0) {
    return (
      <div className="team-board">
        <div className="empty-state">
          <div className="empty-state__icon">
            <ClaudeIcon />
          </div>
          <div className="empty-state__text">No active teams</div>
        </div>
      </div>
    );
  }

  return (
    <div className="team-board">
      {activeTeams.map((team) => (
        <div key={team.name || `team-${team.description}`}>
          <div className="team-board__header">
            <div className="team-board__title">{team.name || "Team"}</div>
            {team.description && <div className="team-board__desc">{team.description}</div>}
          </div>

          <div className="team-board__body">
            {/* Members */}
            {team.members.length > 0 && (
              <div className="team-board__section">
                <div className="team-board__section-title">Members ({team.members.length})</div>
                <div>
                  {team.members.map((member) => {
                    const colorName = team.member_colors[member] ?? "";
                    const color = getTeamColor(colorName);
                    const isOngoing = team.member_ongoing[member] ?? false;

                    return (
                      <span key={member} className="team-member">
                        <span className="team-member__dot" style={{ backgroundColor: color }} />
                        <span className="team-member__name">{member}</span>
                        {isOngoing && <span className="team-member__ongoing" />}
                      </span>
                    );
                  })}
                </div>
              </div>
            )}

            {/* Tasks */}
            {team.tasks.length > 0 && (
              <div className="team-board__section">
                <div className="team-board__section-title">Tasks ({team.tasks.length})</div>
                {team.tasks.map((task) => {
                  const statusIcon = taskStatusIcons[task.status] ?? taskStatusIcons.pending;
                  const statusClass = task.status.replace(/\s+/g, "_");

                  return (
                    <div key={task.id} className="team-task">
                      <span
                        className={`team-task__status-icon team-task__status-icon--${statusClass}`}
                      >
                        {statusIcon}
                      </span>
                      <div className="team-task__content">
                        <div className="team-task__subject">{task.subject}</div>
                        {task.owner && (
                          <div className="team-task__owner">
                            <ForwardIcon /> {task.owner}
                          </div>
                        )}
                      </div>
                      <span
                        className={`team-task__status-badge team-task__status-badge--${statusClass}`}
                      >
                        {task.status}
                      </span>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}
