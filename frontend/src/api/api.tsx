import axios from "axios";

let accessToken: string | null = null;

export function setAccessToken(token: string | null) {
  accessToken = token;
}

export function getAccessToken() {
  return accessToken;
}

axios.defaults.withCredentials = true;
axios.defaults.baseURL = (typeof window !== "undefined"
  ? (import.meta.env.VITE_API_BASE_URL || "backend:5000")
  : import.meta.env.VITE_API_BASE_URL || "backend:5000"
);


axios.interceptors.request.use((config) => {
  if (accessToken) {
    config.headers.Authorization = `Bearer ${accessToken}`;
  }
  return config;
});

export { axios as api };
