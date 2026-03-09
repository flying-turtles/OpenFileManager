import { useState, useCallback } from "react";
import type { Project, ProjectDetail } from "../types";
import * as api from "../api/commands";

export function useProjects() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [selected, setSelected] = useState<ProjectDetail | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      setProjects(await api.getProjects());
    } finally {
      setLoading(false);
    }
  }, []);

  const select = useCallback(async (id: number) => {
    setLoading(true);
    try {
      setSelected(await api.getProjectDetail(id));
    } finally {
      setLoading(false);
    }
  }, []);

  const create = useCallback(
    async (title: string, description: string, startDate: string, endDate: string) => {
      await api.createProject(title, description, startDate, endDate);
      await refresh();
    },
    [refresh]
  );

  const update = useCallback(
    async (id: number, title: string, description: string, startDate: string, endDate: string) => {
      await api.updateProject(id, title, description, startDate, endDate);
      await refresh();
    },
    [refresh]
  );

  const remove = useCallback(
    async (id: number) => {
      await api.deleteProject(id);
      setSelected(null);
      await refresh();
    },
    [refresh]
  );

  return { projects, selected, loading, refresh, select, create, update, remove, setSelected };
}
