FROM node:20-alpine

WORKDIR /app

# Copy package files first for better layer caching
COPY package.json package-lock.json* ./

RUN npm ci --omit=dev

# Copy application source
COPY src/ ./src/

EXPOSE 8787

CMD ["node", "src/index.js"]
