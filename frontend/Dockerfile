FROM node:20-slim AS builder
WORKDIR /app
COPY package.json package-lock.json* ./
RUN npm ci
COPY . .
RUN npm run build

FROM node:20-slim
WORKDIR /app
COPY --from=builder /app/build ./build
COPY --from=builder /app/package.json ./
# Install only production deps; pdfjs worker already bundled in build output
RUN npm ci --omit=dev --ignore-scripts
EXPOSE 3000
ENV NODE_ENV=production
CMD ["node", "build"]
