# end_to_end_test.py
import socket, json, time, struct

def test_mining_flow():
    s = socket.socket()
    s.settimeout(10)
    s.connect(('localhost', 3333))
    
    # Subscribe
    req = {"id": 1, "method": "mining.subscribe", "params": ["test/1.0"]}
    s.send((json.dumps(req) + '\n').encode())
    response = s.recv(4096).decode()
    print(f"1. Subscribe: {response[:100]}")
    
    # Authorize
    req = {"id": 2, "method": "mining.authorize", "params": ["test.worker", ""]}
    s.send((json.dumps(req) + '\n').encode())
    response = s.recv(4096).decode()
    print(f"2. Authorize: {response}")
    
    # Wait for mining.notify (the actual work)
    time.sleep(5)
    response = s.recv(4096).decode()
    print(f"3. Work notify: {response[:200]}")
    
    if "mining.notify" not in response:
        print("FAILED: No work received")
        return False
    
    print("SUCCESS: Received mining work")
    return True

test_mining_flow()