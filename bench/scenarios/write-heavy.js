// k6 scenario: Write-heavy workload (90% writes, 10% reads)

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend, Counter } from 'k6/metrics';

// Custom metrics
const writeSuccess = new Rate('write_success');
const readSuccess = new Rate('read_success');
const writeLatency = new Trend('write_latency');
const readLatency = new Trend('read_latency');
const bytesWritten = new Counter('bytes_written');

const BASE_URL = __ENV.BASE_URL || 'http://127.0.0.1:5000';
const OBJECT_SIZE = parseInt(__ENV.OBJECT_SIZE || '1048576'); // 1 MB
const WRITE_RATIO = 0.9; // 90% writes

export let options = {
    stages: [
        { duration: '30s', target: 10 },  // Ramp up
        { duration: '2m', target: 50 },   // Steady state
        { duration: '30s', target: 0 },   // Ramp down
    ],
    thresholds: {
        'write_success': ['rate>0.85'],
        'read_success': ['rate>0.95'],
        'write_latency': ['p(95)<1000'],
        'read_latency': ['p(95)<200'],
    },
};

function generateData(size) {
    const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
    let result = '';
    for (let i = 0; i < size; i++) {
        result += chars.charAt(Math.floor(Math.random() * chars.length));
    }
    return result;
}

let writtenKeys = [];

export default function () {
    const shouldWrite = Math.random() < WRITE_RATIO;
    
    if (shouldWrite) {
        // WRITE
        const key = `write-heavy-${__VU}-${__ITER}`;
        const data = generateData(OBJECT_SIZE);
        
        const start = Date.now();
        const res = http.put(`${BASE_URL}/${key}`, data, {
            headers: { 'Content-Type': 'application/octet-stream' },
        });
        const duration = Date.now() - start;
        
        writeSuccess.add(res.status === 201 || res.status === 501);
        writeLatency.add(duration);
        bytesWritten.add(OBJECT_SIZE);
        
        check(res, {
            'write ok': (r) => r.status === 201 || r.status === 501,
        });
        
        if (res.status === 201) {
            writtenKeys.push(key);
        }
    } else {
        // READ (from previously written keys)
        if (writtenKeys.length > 0) {
            const key = writtenKeys[Math.floor(Math.random() * writtenKeys.length)];
            
            const start = Date.now();
            const res = http.get(`${BASE_URL}/${key}`);
            const duration = Date.now() - start;
            
            readSuccess.add(res.status === 200);
            readLatency.add(duration);
            
            check(res, {
                'read ok': (r) => r.status === 200,
            });
        }
    }
    
    sleep(0.1);
}
