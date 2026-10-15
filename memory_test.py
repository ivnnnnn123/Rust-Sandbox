import time
data = [bytearray(20_000_000) for _ in range(10)]  # 200MB
time.sleep(0.2)  # dă timp monitorizării
while True:
    data.append(bytearray(10_000_000))
    time.sleep(0.05)