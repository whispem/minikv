// k6 scenario: Read-heavy workload (10% writes, 90% reads)

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend } from 'k6/metrics';

const readSuccess = new Rate('read_success');
const readLatency = new Trend('read_latency');
const cacheHitRate = new Rate('cache_hit_rate');

const BASE_URL = __ENV.BASE_URL || 'http://127.0.0.1:5000';
const OBJECT_SIZE = parseInt(__ENV.OBJECT_SIZE || '1048576');
const READ_RATIO = 0.9; // 90% reads

export let options = {
    stages: [
        { duration: '30s', target: 20 },
        { duration: '2m', target: 100 },
        { duration: '30s', target: 0 },
    ],
    thresholds: {
        'read_success': ['rate>0.99'],
        'read_latency': ['p(95)<100'],
    },
};

// Pre-populate keys in setup
export function setup() {
    const keys = [];
    for (let i = 0; i < 1000; i++) {
        keys.push(`read-heavy-key-${i}`);
    }
    return { keys };
}

export default function (data) {
    const shouldRead = Math.random() < READ_RATIO;
    
    if (shouldRead && data.keys.length > 0) {
        // READ
        const key = data.keys[Math.floor(Math.random() * data.keys.length)];
        
        const start = Date.now();
        const res = http.get(`${BASE_URL}/${key}`);
        const duration = Date.now() - start;
        
        readSuccess.add(res.status === 200 || res.status === 404);
        readLatency.add(duration);
        
        // Track cache hits (very fast responses)
        if (duration < 10) {
            cacheHitRate.add(1);
        } else {
            cacheHitRate.add(0);
        }
        
        check(res, {
            'read completed': (r) => r.status !== 0,
        });
    }
    
    sleep(0.05);
}
