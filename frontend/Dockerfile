# Build stage
FROM node:22-alpine AS builder
# arg and env
ARG VITE_API_BASE_URL
ENV VITE_API_BASE_URL=${VITE_API_BASE_URL}
ENV DEV=true
# end args
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY . .
# RUN mkdir -p public

RUN npm run build

# Production stage
FROM nginx:alpine
COPY --from=builder /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/nginx.conf
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]

# FROM node:22-alpine AS runner
# RUN addgroup --system --gid 1001 nodejs
# RUN adduser --system --uid 1001 nextjs
# COPY --from=builder /app/public ./public
# # COPY --from=builder --chown=nextjs:nodejs /app/.next/standalone ./
# # COPY --from=builder --chown=nextjs:nodejs /app/.next/static ./.next/static
# USER nextjs
# EXPOSE 3000
# ENV PORT=3000
# ENV HOSTNAME="0.0.0.0"
# CMD ["npm", "run" , "dev"]
