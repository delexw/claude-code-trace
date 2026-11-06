import { describe, expect, it } from "vitest";
import { buildCodexProjectItems, codexProjectKey, filterCodexProjectSessions } from "./projects";
import { makeSession } from "./testFixtures";

describe("Codex project folders", () => {
  it("groups by real folder and retains distinct same-name projects", () => {
    const a = makeSession({ id: "a", cwd: "/work/one/app" });
    const a2 = makeSession({ id: "a2", cwd: "/work/one/app/" });
    const b = makeSession({ id: "b", cwd: "/work/two/app" });
    const items = buildCodexProjectItems([a, a2, b]);
    expect(items[0]).toMatchObject({ key: null, name: "All Projects", count: 3 });
    expect(items.slice(1).map((item) => item.count)).toEqual([2, 1]);
    expect(filterCodexProjectSessions([a, a2, b], codexProjectKey(a))).toEqual([a, a2]);
    expect(codexProjectKey(a)).not.toBe(codexProjectKey(b));
  });

  it("normalizes Windows separators/case while preserving POSIX case and WSL distro", () => {
    expect(codexProjectKey(makeSession({ cwd: "D:\\Work\\App\\" }))).toBe(
      codexProjectKey(makeSession({ cwd: "d:/work/app" })),
    );
    expect(codexProjectKey(makeSession({ cwd: "/work/App" }))).not.toBe(
      codexProjectKey(makeSession({ cwd: "/work/app" })),
    );
    const ubuntu = makeSession({ cwd: "/home/demo/app", source: "WSL: ExampleUbuntu" });
    const debian = makeSession({ cwd: "/home/demo/app", source: "WSL: ExampleDebian" });
    expect(codexProjectKey(ubuntu)).not.toBe(codexProjectKey(debian));
    expect(codexProjectKey(ubuntu)).toBe(
      codexProjectKey(makeSession({ cwd: "\\\\wsl.localhost\\ExampleUbuntu\\home\\demo\\app" })),
    );
  });

  it("nests only real descendants, supports collapse, and filters exact projects", () => {
    const parent = makeSession({ cwd: "/work/app" });
    const child = makeSession({ id: "child", cwd: "/work/app/tools" });
    const unrelated = makeSession({ id: "other", cwd: "/work/app-other" });
    const sessions = [parent, child, unrelated];
    const items = buildCodexProjectItems(sessions);
    expect(items.find((item) => item.key === codexProjectKey(parent))).toMatchObject({
      depth: 0,
      count: 1,
      hasChildren: true,
    });
    expect(items.find((item) => item.key === codexProjectKey(child))).toMatchObject({ depth: 1 });
    expect(items.find((item) => item.key === codexProjectKey(unrelated))).toMatchObject({
      depth: 0,
    });
    expect(buildCodexProjectItems(sessions, new Set([codexProjectKey(parent)]))).not.toContainEqual(
      expect.objectContaining({ key: codexProjectKey(child) }),
    );
    expect(filterCodexProjectSessions(sessions, codexProjectKey(parent))).toEqual([parent]);
  });

  it("keeps missing-folder conversations selectable", () => {
    const unknown = makeSession({ cwd: "" });
    const items = buildCodexProjectItems([unknown]);
    expect(items[1]).toMatchObject({ name: "Unknown folder", count: 1 });
    expect(filterCodexProjectSessions([unknown], items[1].key)).toEqual([unknown]);
  });

  it("groups user forks by their parent's stable origin and guards missing or cyclic parents", () => {
    const parent = {
      ...makeSession({ id: "parent", cwd: "/work/elsewhere" }),
      dirs: ["/work/origin"],
    };
    const fork = {
      ...makeSession({ id: "fork", cwd: "/work/fork" }),
      forked_from_session_id: "parent",
      is_ongoing: true,
    };
    const items = buildCodexProjectItems([parent, fork]);
    expect(items).toHaveLength(2);
    expect(items[1]).toMatchObject({ name: "origin", count: 2, ongoing: true });
    expect(filterCodexProjectSessions([parent, fork], codexProjectKey(parent))).toEqual([
      parent,
      fork,
    ]);
    expect(buildCodexProjectItems([fork])[1].name).toBe("fork");
    expect(() =>
      buildCodexProjectItems([{ ...parent, forked_from_session_id: "fork" }, fork]),
    ).not.toThrow();
  });
});
