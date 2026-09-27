from abc import ABC, abstractmethod


class Animal(ABC):
    def __init__(self, nombre: str, edad: int) -> None:
        self.nombre = nombre
        self.edad = edad

    @abstractmethod
    def sonido(self) -> str:
        ...

    def presentarse(self) -> str:
        return f"{self.nombre} ({self.edad} años) dice {self.sonido()}"
